from __future__ import annotations

import hashlib
import importlib
import os
import sqlite3
from collections.abc import Iterable, Iterator, Mapping
from contextlib import suppress
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, unquote, urlparse

from tools.vault_sync.config import DatabaseImportConfig, VaultConfig, expand_path
from tools.vault_sync.io_utils import (
    atomic_write_json,
    atomic_write_text,
    load_json,
    resolve_vault_path,
    safe_filename,
    sha256_json,
)
from tools.vault_sync.markdown import normalize_markdown, yaml_scalar


class DatabaseImportError(RuntimeError):
    pass


@dataclass
class DatabaseImportResult:
    source: str
    rows_read: int = 0
    imported: int = 0
    updated: int = 0
    removed: int = 0
    skipped: int = 0
    failed: int = 0
    warnings: list[str] = field(default_factory=list)


def import_database(vault: VaultConfig, config: DatabaseImportConfig) -> DatabaseImportResult:
    result = DatabaseImportResult(source=config.name)
    if not config.enabled:
        return result
    _validate_query(config.query)
    destination = resolve_vault_path(vault.root, config.destination)
    previous = load_json(config.manifest, {"version": 1, "entries": {}})
    previous_entries = previous.get("entries", {}) if isinstance(previous, dict) else {}
    current_entries: dict[str, dict[str, Any]] = {}
    seen_ids: set[str] = set()

    for row in _iter_rows(config):
        result.rows_read += 1
        row_id = str(row.get(config.id_column, "")).strip()
        if not row_id:
            result.failed += 1
            result.warnings.append(f"row {result.rows_read}: missing id column {config.id_column!r}")
            continue
        if row_id in seen_ids:
            result.failed += 1
            result.warnings.append(f"duplicate row id {row_id!r}; query must return stable unique ids")
            continue
        seen_ids.add(row_id)
        row_hash = sha256_json(row)
        output_relative = _row_output(row_id)
        output_path = destination / output_relative
        previous_entry = previous_entries.get(row_id, {})
        if previous_entry.get("sha256") == row_hash and output_path.exists():
            current_entries[row_id] = previous_entry
            result.skipped += 1
            continue
        try:
            atomic_write_text(output_path, _render_row(config, row_id, row))
        except (OSError, UnicodeError, ValueError) as error:
            result.failed += 1
            result.warnings.append(f"row {row_id!r}: {error}")
            if previous_entry:
                current_entries[row_id] = previous_entry
            continue
        current_entries[row_id] = {
            "sha256": row_hash,
            "output": output_relative.as_posix(),
            "updated_at": _string_value(row.get(config.updated_at_column)) if config.updated_at_column else None,
        }
        if previous_entry:
            result.updated += 1
        else:
            result.imported += 1

    for row_id, entry in previous_entries.items():
        if row_id in current_entries:
            continue
        if not config.delete_removed:
            current_entries[row_id] = entry
            continue
        output_value = entry.get("output") if isinstance(entry, dict) else None
        if output_value:
            (destination / output_value).unlink(missing_ok=True)
        result.removed += 1

    atomic_write_json(
        config.manifest,
        {
            "version": 1,
            "driver": config.driver,
            "source": config.name,
            "query_sha256": hashlib.sha256(config.query.encode("utf-8")).hexdigest(),
            "entries": current_entries,
        },
    )
    return result


def _validate_query(query: str) -> None:
    normalized = query.lstrip().lower()
    while normalized.startswith("--"):
        normalized = normalized.partition("\n")[2].lstrip()
    if not (normalized.startswith("select") or normalized.startswith("with")):
        raise DatabaseImportError("database imports only accept read-only SELECT or WITH queries")


def _iter_rows(config: DatabaseImportConfig) -> Iterator[dict[str, Any]]:
    if config.driver == "sqlite":
        yield from _iter_sqlite(config)
        return
    if config.driver in {"postgres", "postgresql"}:
        yield from _iter_dbapi(config, "psycopg")
        return
    if config.driver in {"mysql", "mariadb"}:
        yield from _iter_dbapi(config, "pymysql")
        return
    raise DatabaseImportError(f"unsupported database driver: {config.driver}")


def _iter_sqlite(config: DatabaseImportConfig) -> Iterator[dict[str, Any]]:
    if not config.database:
        raise DatabaseImportError("sqlite database import requires `database`")
    path = expand_path(config.database)
    if not path.is_file():
        raise DatabaseImportError(f"sqlite database does not exist: {path}")
    connection = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        connection.execute("PRAGMA query_only = ON")
        cursor = connection.execute(config.query)
        for row in cursor:
            yield dict(row)
    except sqlite3.Error as error:
        raise DatabaseImportError(f"sqlite query failed: {error}") from error
    finally:
        connection.close()


def _iter_dbapi(config: DatabaseImportConfig, module_name: str) -> Iterator[dict[str, Any]]:
    if not config.dsn_env:
        raise DatabaseImportError(f"{config.driver} database import requires `dsn_env`")
    dsn = os.getenv(config.dsn_env)
    if not dsn:
        raise DatabaseImportError(f"environment variable {config.dsn_env} is not set")
    try:
        module = importlib.import_module(module_name)
    except ImportError as error:
        raise DatabaseImportError(
            f"{config.driver} support requires the optional Python package {module_name!r}"
        ) from error
    try:
        connection = _connect_dbapi(module, module_name, dsn)
        cursor = connection.cursor()
        if module_name == "psycopg":
            cursor.execute("BEGIN READ ONLY")
        elif module_name == "pymysql":
            cursor.execute("START TRANSACTION READ ONLY")
        cursor.execute(config.query)
        columns = [description[0] for description in cursor.description or []]
        while rows := cursor.fetchmany(500):
            for row in rows:
                if isinstance(row, Mapping):
                    yield dict(row)
                else:
                    yield dict(zip(columns, row, strict=False))
    except Exception as error:
        raise DatabaseImportError(f"{config.driver} query failed: {error}") from error
    finally:
        if "cursor" in locals():
            cursor.close()
        if "connection" in locals():
            with suppress(Exception):
                connection.rollback()
            connection.close()


def _connect_dbapi(module: Any, module_name: str, dsn: str) -> Any:
    if module_name != "pymysql":
        return module.connect(dsn)
    parsed = urlparse(dsn)
    if parsed.scheme not in {"mysql", "mariadb"} or not parsed.hostname:
        raise DatabaseImportError(
            "mysql DSN must be a mysql:// or mariadb:// URL stored in the configured environment variable"
        )
    options = parse_qs(parsed.query)
    arguments: dict[str, Any] = {
        "host": parsed.hostname,
        "port": parsed.port or 3306,
        "user": unquote(parsed.username or ""),
        "password": unquote(parsed.password or ""),
        "database": unquote(parsed.path.lstrip("/")),
        "charset": options.get("charset", ["utf8mb4"])[0],
    }
    if unix_socket := options.get("unix_socket", [None])[0]:
        arguments["unix_socket"] = unix_socket
    return module.connect(**arguments)


def _row_output(row_id: str) -> Path:
    suffix = hashlib.sha256(row_id.encode("utf-8")).hexdigest()[:10]
    return Path(f"{safe_filename(row_id)}--{suffix}.md")


def _render_row(config: DatabaseImportConfig, row_id: str, row: Mapping[str, Any]) -> str:
    title_value = row.get(config.title_column) if config.title_column else None
    title = _string_value(title_value).strip() or f"{config.name} {row_id}"
    metadata = _selected_values(row, config.metadata_columns)
    content_columns = config.content_columns or [
        key
        for key in row
        if key not in {config.id_column, config.title_column, *config.metadata_columns}
    ]
    lines = [
        "---",
        f"title: {yaml_scalar(title)}",
        f"memento_source: {yaml_scalar(f'database://{config.name}/{row_id}')} ",
        f"memento_source_type: {yaml_scalar('database')}",
        f"memento_database_driver: {yaml_scalar(config.driver)}",
        f"memento_database_id: {yaml_scalar(row_id)}",
        f"tags: [{', '.join(yaml_scalar(tag) for tag in [*config.tags, config.name])}]",
    ]
    for key, value in metadata.items():
        lines.append(f"db_{_frontmatter_key(key)}: {yaml_scalar(_string_value(value))}")
    lines.extend(["---", "", f"# {title}", ""])
    if metadata:
        lines.extend(["## Metadata", ""])
        lines.extend(f"- **{key}:** {_string_value(value)}" for key, value in metadata.items())
        lines.append("")
    for column in content_columns:
        if column not in row or row[column] is None:
            continue
        lines.extend([f"## {column.replace('_', ' ').title()}", "", _markdown_value(row[column]), ""])
    return normalize_markdown("\n".join(lines))


def _selected_values(row: Mapping[str, Any], columns: Iterable[str]) -> dict[str, Any]:
    return {column: row[column] for column in columns if column in row and row[column] is not None}


def _string_value(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return f"<binary {len(value)} bytes>"
    return str(value)


def _markdown_value(value: Any) -> str:
    if isinstance(value, bytes):
        return f"_Binary value omitted ({len(value)} bytes)._"
    if isinstance(value, (dict, list, tuple)):
        import json

        return f"```json\n{json.dumps(value, indent=2, ensure_ascii=False, default=str)}\n```"
    return _string_value(value)


def _frontmatter_key(value: str) -> str:
    normalized = "".join(char.lower() if char.isalnum() else "_" for char in value)
    return "_".join(part for part in normalized.split("_") if part) or "value"

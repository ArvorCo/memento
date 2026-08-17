from __future__ import annotations

import os
import shutil
from dataclasses import dataclass, field
from pathlib import Path

from tools.vault_sync.config import DocumentImportConfig, VaultConfig
from tools.vault_sync.document_converter import ConversionError, convert_document
from tools.vault_sync.io_utils import (
    atomic_write_json,
    atomic_write_text,
    load_json,
    resolve_vault_path,
    sha256_file,
)
from tools.vault_sync.markdown import augment_markdown_source, markdown_title, render_imported_markdown


@dataclass
class DocumentImportResult:
    source: str
    discovered: int = 0
    imported: int = 0
    updated: int = 0
    removed: int = 0
    skipped: int = 0
    failed: int = 0
    warnings: list[str] = field(default_factory=list)


def import_documents(vault: VaultConfig, config: DocumentImportConfig) -> DocumentImportResult:
    result = DocumentImportResult(source=config.name)
    if not config.enabled:
        return result
    destination_root = resolve_vault_path(vault.root, config.destination)
    raw_root = resolve_vault_path(vault.root, config.raw_destination) if config.raw_destination else None
    source_files = _discover_files(config)
    result.discovered = len(source_files)
    previous = load_json(config.manifest, {"version": 1, "entries": {}})
    previous_entries = previous.get("entries", {}) if isinstance(previous, dict) else {}
    current_entries: dict[str, dict] = {}

    for relative_name, source_path in source_files.items():
        previous_entry = previous_entries.get(relative_name, {})
        try:
            size = source_path.stat().st_size
            if size > config.max_file_bytes:
                result.failed += 1
                result.warnings.append(f"{relative_name}: exceeds max_file_bytes ({size})")
                continue
            source_hash = sha256_file(source_path)
            output_relative = _output_relative(Path(relative_name))
            output_path = destination_root / output_relative
            if previous_entry.get("sha256") == source_hash and output_path.exists():
                current_entries[relative_name] = previous_entry
                result.skipped += 1
                continue

            conversion = convert_document(source_path)
            title = markdown_title(source_path, conversion.markdown)
            source_label = source_path.resolve().as_uri()
            if source_path.suffix.lower() in {".md", ".markdown", ".mdx"}:
                rendered = augment_markdown_source(
                    content=conversion.markdown,
                    title=title,
                    source=source_label,
                    source_hash=source_hash,
                    source_type=conversion.source_type,
                    converter=conversion.converter,
                    tags=config.tags,
                )
            else:
                rendered = render_imported_markdown(
                    title=title,
                    body=conversion.markdown,
                    source=source_label,
                    source_hash=source_hash,
                    source_type=conversion.source_type,
                    converter=conversion.converter,
                    tags=[*config.tags, conversion.source_type],
                    warnings=conversion.warnings,
                )
            atomic_write_text(output_path, rendered)
            if config.preserve_raw and raw_root:
                raw_path = raw_root / relative_name
                raw_path.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source_path, raw_path)
            current_entries[relative_name] = {
                "sha256": source_hash,
                "size": size,
                "output": output_relative.as_posix(),
                "converter": conversion.converter,
            }
            if previous_entry:
                result.updated += 1
            else:
                result.imported += 1
            result.warnings.extend(f"{relative_name}: {warning}" for warning in conversion.warnings)
        except (ConversionError, OSError, UnicodeError, ValueError) as error:
            result.failed += 1
            result.warnings.append(f"{relative_name}: {error}")
            if previous_entry:
                current_entries[relative_name] = previous_entry

    for relative_name, entry in previous_entries.items():
        if relative_name in current_entries:
            continue
        if relative_name in source_files or not config.delete_removed:
            current_entries[relative_name] = entry
            continue
        output_value = entry.get("output") if isinstance(entry, dict) else None
        if output_value:
            (destination_root / output_value).unlink(missing_ok=True)
        if raw_root:
            (raw_root / relative_name).unlink(missing_ok=True)
        result.removed += 1

    atomic_write_json(
        config.manifest,
        {
            "version": 1,
            "source": config.source.as_posix(),
            "destination": config.destination.as_posix(),
            "entries": current_entries,
        },
    )
    return result


def _discover_files(config: DocumentImportConfig) -> dict[str, Path]:
    if config.source.is_file():
        if config.source.suffix.lower() not in config.include_extensions:
            return {}
        return {config.source.name: config.source}
    if not config.source.exists():
        return {}
    files: dict[str, Path] = {}
    for current_root, directories, filenames in os.walk(config.source):
        directories[:] = sorted(name for name in directories if name not in config.exclude_dirs)
        current = Path(current_root)
        for filename in sorted(filenames):
            path = current / filename
            if path.is_symlink() or path.suffix.lower() not in config.include_extensions:
                continue
            files[path.relative_to(config.source).as_posix()] = path
    return files


def _output_relative(source_relative: Path) -> Path:
    if source_relative.suffix.lower() in {".md", ".markdown", ".mdx"}:
        return source_relative.with_suffix(".md")
    return source_relative.with_name(f"{source_relative.name}.md")

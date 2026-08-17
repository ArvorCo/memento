#!/usr/bin/env -S uv run python
from __future__ import annotations

import argparse
import contextlib
import io
import json
import sys
from collections.abc import Callable
from dataclasses import asdict
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.vault_sync.apple_notes import export_apple_notes
from tools.vault_sync.config import load_config
from tools.vault_sync.database_import import DatabaseImportError, import_database
from tools.vault_sync.document_converter import conversion_capabilities
from tools.vault_sync.document_import import import_documents
from tools.vault_sync.filesystem_sync import sync_markdown_root
from tools.vault_sync.icloud_sync import sync_icloud
from tools.vault_sync.presets import detect_preset_name, write_preset
from tools.vault_sync.session_importers import (
    import_chatgpt,
    import_claude,
    import_codex,
    import_droid,
)
from tools.vault_sync.whatsapp_import import import_whatsapp
from tools.vault_sync.wiki_linker import link_vault


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Generic vault sync toolkit for Memento.")
    parser.add_argument("--config", help="Path to TOML config", default=None)
    parser.add_argument("--json", action="store_true", help="Emit compact machine-readable JSON.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("sync-markdown", help="Sync configured markdown roots into the vault.")
    subparsers.add_parser("sync-icloud", help="Sync configured iCloud folders into the vault.")
    subparsers.add_parser(
        "export-apple-notes",
        help="Export Apple Notes into the vault using the configured connector.",
    )
    subparsers.add_parser("import-whatsapp", help="Import WhatsApp ZIP exports into the vault.")
    documents_parser = subparsers.add_parser(
        "import-documents", help="Discover and convert configured document sources into Markdown."
    )
    documents_parser.add_argument("source", nargs="?", default="all", help="Configured source name or all.")
    databases_parser = subparsers.add_parser(
        "import-databases", help="Import configured read-only database queries into Markdown."
    )
    databases_parser.add_argument("source", nargs="?", default="all", help="Configured source name or all.")
    subparsers.add_parser("link-vault", help="Build directory/topic hubs and idempotent wiki navigation.")
    subparsers.add_parser("capabilities", help="Report local conversion and connector capabilities.")
    subparsers.add_parser(
        "run-all",
        help="Run the configured markdown sync plus all enabled session importers.",
    )
    init_parser = subparsers.add_parser(
        "init-config",
        help="Write a starter vault sync config for macOS, Linux, or Windows.",
    )
    init_parser.add_argument(
        "--preset",
        choices=["auto", "mac", "linux", "windows"],
        default="auto",
        help="Platform preset to use.",
    )
    init_parser.add_argument(
        "--output",
        default=None,
        help="Output TOML path. Defaults to ./memento-vault-sync.toml",
    )
    init_parser.add_argument(
        "--vault-root",
        default=None,
        help="Override vault root path in the generated config.",
    )
    init_parser.add_argument(
        "--state-dir",
        default=None,
        help="Override state dir in the generated config.",
    )
    init_parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite an existing file.",
    )

    import_parser = subparsers.add_parser(
        "import-sessions",
        help="Import AI session exports into the vault.",
    )
    import_parser.add_argument("connector", choices=["all", "codex", "droid", "claude", "chatgpt"])
    return parser


def run_sync_markdown(config_path: str | None) -> int:
    config = load_config(config_path)
    failed = 0
    for root in config.markdown_roots:
        result = sync_markdown_root(config.vault, root)
        print(
            f"[{root.name}] copied={result.copied} deleted={result.deleted} "
            f"skipped={result.skipped} failed={result.failed}"
        )
        failed += result.failed
    return 0 if failed == 0 else 1


def run_sync_icloud(config_path: str | None) -> int:
    config = load_config(config_path)
    if not config.icloud or not config.icloud.enabled:
        print("[icloud] skipped (disabled or missing in config)")
        return 0
    result = sync_icloud(config.vault, config.icloud)
    print(
        f"[icloud] copied={result.copied} converted={result.converted} skipped={result.skipped} failed={result.failed}"
    )
    for warning in result.warnings:
        print(f"[icloud] warning: {warning}", file=sys.stderr)
    return 0 if result.failed == 0 else 1


def run_export_apple_notes(config_path: str | None) -> int:
    config = load_config(config_path)
    if not config.apple_notes or not config.apple_notes.enabled:
        print("[apple-notes] skipped (disabled or missing in config)")
        return 0
    result = export_apple_notes(config.vault, config.apple_notes)
    print(
        f"[apple-notes] exported={result.exported} converted={result.converted} failed={result.failed}"
        + (f" index={result.index_path}" if result.index_path else "")
    )
    return 0 if result.failed == 0 else 1


def run_import_whatsapp(config_path: str | None) -> int:
    config = load_config(config_path)
    if not config.whatsapp or not config.whatsapp.enabled:
        print("[whatsapp] skipped (disabled or missing in config)")
        return 0
    result = import_whatsapp(config.vault, config.whatsapp)
    print(
        f"[whatsapp] imported={result.imported} skipped={result.skipped} "
        f"messages={result.messages} media={result.media_files} failed={result.failed}"
    )
    return 0 if result.failed == 0 else 1


def run_import_sessions(config_path: str | None, connector_name: str) -> int:
    config = load_config(config_path)
    connector_map = {
        "codex": import_codex,
        "droid": import_droid,
        "claude": import_claude,
        "chatgpt": import_chatgpt,
    }
    selected = connector_map.keys() if connector_name == "all" else [connector_name]
    for name in selected:
        connector = config.session_imports.get(name)
        if not connector or not connector.enabled:
            print(f"[{name}] skipped (disabled or missing in config)")
            continue
        result = connector_map[name](config.vault, connector, config.linking)
        print(f"[{result.connector}] imported={result.imported} skipped={result.skipped} hub={result.hub_path}")
    return 0


def run_import_documents(config_path: str | None, source_name: str, json_output: bool = False) -> int:
    config = load_config(config_path)
    selected = [item for item in config.document_imports if source_name == "all" or item.name == source_name]
    if source_name != "all" and not selected:
        print(f"unknown document source: {source_name}", file=sys.stderr)
        return 2
    results = [import_documents(config.vault, item) for item in selected]
    if json_output:
        print(json.dumps({"command": "import-documents", "results": [asdict(item) for item in results]}, default=str))
    else:
        for result in results:
            print(
                f"[{result.source}] discovered={result.discovered} imported={result.imported} "
                f"updated={result.updated} removed={result.removed} skipped={result.skipped} failed={result.failed}"
            )
            for warning in result.warnings:
                print(f"[{result.source}] warning: {warning}", file=sys.stderr)
    return 0 if all(result.failed == 0 for result in results) else 1


def run_import_databases(config_path: str | None, source_name: str, json_output: bool = False) -> int:
    config = load_config(config_path)
    selected = [item for item in config.database_imports if source_name == "all" or item.name == source_name]
    if source_name != "all" and not selected:
        print(f"unknown database source: {source_name}", file=sys.stderr)
        return 2
    results = []
    for item in selected:
        try:
            results.append(import_database(config.vault, item))
        except DatabaseImportError as error:
            if json_output:
                print(json.dumps({"command": "import-databases", "error": str(error), "source": item.name}))
            else:
                print(f"[{item.name}] error: {error}", file=sys.stderr)
            return 1
    if json_output:
        print(json.dumps({"command": "import-databases", "results": [asdict(item) for item in results]}, default=str))
    else:
        for result in results:
            print(
                f"[{result.source}] rows={result.rows_read} imported={result.imported} updated={result.updated} "
                f"removed={result.removed} skipped={result.skipped} failed={result.failed}"
            )
            for warning in result.warnings:
                print(f"[{result.source}] warning: {warning}", file=sys.stderr)
    return 0 if all(result.failed == 0 for result in results) else 1


def run_link_vault(config_path: str | None, json_output: bool = False) -> int:
    config = load_config(config_path)
    result = link_vault(config.vault, config.linking)
    if json_output:
        print(json.dumps({"command": "link-vault", "result": asdict(result)}, default=str))
    else:
        print(
            f"[linker] documents={result.documents} directory_hubs={result.directory_hubs} "
            f"tag_hubs={result.tag_hubs} navigation_updated={result.navigation_updated} "
            f"unchanged={result.unchanged} failed={result.failed}"
        )
        for warning in result.warnings:
            print(f"[linker] warning: {warning}", file=sys.stderr)
    return 0 if result.failed == 0 else 1


def run_capabilities(config_path: str | None, json_output: bool = False) -> int:
    config = load_config(config_path)
    payload = {
        "conversion": conversion_capabilities(),
        "document_sources": [item.name for item in config.document_imports if item.enabled],
        "database_sources": [item.name for item in config.database_imports if item.enabled],
        "linker_enabled": config.linking.enabled,
    }
    if json_output:
        print(json.dumps(payload))
    else:
        for name, available in payload["conversion"].items():
            print(f"[conversion] {name}={'yes' if available else 'no'}")
        print(f"[sources] documents={len(payload['document_sources'])} databases={len(payload['database_sources'])}")
        print(f"[linker] enabled={'yes' if config.linking.enabled else 'no'}")
    return 0


def _capture_command(name: str, callback: Callable[[], int]) -> dict[str, object]:
    stdout = io.StringIO()
    stderr = io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        status = callback()
    return {
        "name": name,
        "status": status,
        "stdout": stdout.getvalue().splitlines(),
        "stderr": stderr.getvalue().splitlines(),
    }


def _emit_captured_json(name: str, callback: Callable[[], int]) -> int:
    result = _capture_command(name, callback)
    print(json.dumps({"command": name, **result}))
    return int(result["status"])


def run_all(config_path: str | None, json_output: bool = False) -> int:
    steps = [
        ("sync-markdown", lambda: run_sync_markdown(config_path)),
        ("import-documents", lambda: run_import_documents(config_path, "all")),
        ("import-databases", lambda: run_import_databases(config_path, "all")),
        ("sync-icloud", lambda: run_sync_icloud(config_path)),
        ("export-apple-notes", lambda: run_export_apple_notes(config_path)),
        ("import-whatsapp", lambda: run_import_whatsapp(config_path)),
        ("import-sessions", lambda: run_import_sessions(config_path, "all")),
        ("link-vault", lambda: run_link_vault(config_path)),
    ]
    if json_output:
        results = [_capture_command(name, callback) for name, callback in steps]
        status = 0 if all(result["status"] == 0 for result in results) else 1
        print(json.dumps({"command": "run-all", "status": status, "steps": results}))
        return status
    statuses = [callback() for _, callback in steps]
    return 0 if all(status == 0 for status in statuses) else 1


def run_init_config(
    *,
    preset_name: str,
    output: str | None,
    vault_root: str | None,
    state_dir: str | None,
    force: bool,
) -> int:
    resolved_preset = detect_preset_name() if preset_name == "auto" else preset_name
    output_path = Path(output) if output else Path.cwd() / "memento-vault-sync.toml"
    if output_path.exists() and not force:
        print(f"config already exists at {output_path}; use --force to overwrite")
        return 1
    write_preset(
        output_path,
        preset_name=resolved_preset,
        vault_root=vault_root,
        state_dir=state_dir,
    )
    print(f"wrote {output_path} using preset={resolved_preset}")
    return 0


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    if args.command == "sync-markdown":
        if args.json:
            return _emit_captured_json(args.command, lambda: run_sync_markdown(args.config))
        return run_sync_markdown(args.config)
    if args.command == "sync-icloud":
        if args.json:
            return _emit_captured_json(args.command, lambda: run_sync_icloud(args.config))
        return run_sync_icloud(args.config)
    if args.command == "export-apple-notes":
        if args.json:
            return _emit_captured_json(args.command, lambda: run_export_apple_notes(args.config))
        return run_export_apple_notes(args.config)
    if args.command == "import-whatsapp":
        if args.json:
            return _emit_captured_json(args.command, lambda: run_import_whatsapp(args.config))
        return run_import_whatsapp(args.config)
    if args.command == "import-documents":
        return run_import_documents(args.config, args.source, args.json)
    if args.command == "import-databases":
        return run_import_databases(args.config, args.source, args.json)
    if args.command == "link-vault":
        return run_link_vault(args.config, args.json)
    if args.command == "capabilities":
        return run_capabilities(args.config, args.json)
    if args.command == "run-all":
        return run_all(args.config, args.json)
    if args.command == "init-config":
        return run_init_config(
            preset_name=args.preset,
            output=args.output,
            vault_root=args.vault_root,
            state_dir=args.state_dir,
            force=args.force,
        )
    if args.command == "import-sessions":
        if args.json:
            return _emit_captured_json(
                args.command, lambda: run_import_sessions(args.config, args.connector)
            )
        return run_import_sessions(args.config, args.connector)
    parser.error("unknown command")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())

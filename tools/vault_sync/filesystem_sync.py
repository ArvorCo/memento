from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from tools.vault_sync.config import MarkdownSyncRoot, VaultConfig
from tools.vault_sync.io_utils import atomic_copy, atomic_write_json, load_json, resolve_vault_path, sha256_file


@dataclass
class MarkdownSyncResult:
    copied: int = 0
    deleted: int = 0
    skipped: int = 0
    failed: int = 0


def should_include(path: Path, root: MarkdownSyncRoot) -> bool:
    return path.suffix.lower() in root.include_extensions


def collect_source_files(root: MarkdownSyncRoot) -> dict[str, Path]:
    files: dict[str, Path] = {}
    for current_root, dirs, filenames in __import__("os").walk(root.source):
        dirs[:] = [name for name in dirs if name not in root.exclude_dirs]
        current_path = Path(current_root)
        for filename in filenames:
            full_path = current_path / filename
            if not should_include(full_path, root):
                continue
            relative_path = full_path.relative_to(root.source).as_posix()
            files[relative_path] = full_path
    return files


def sync_markdown_root(vault: VaultConfig, root: MarkdownSyncRoot) -> MarkdownSyncResult:
    result = MarkdownSyncResult()
    destination_root = resolve_vault_path(vault.root, root.destination)
    destination_root.mkdir(parents=True, exist_ok=True)
    manifest_path = root.manifest or vault.state_dir / f"markdown-{root.name}.json"
    previous = load_json(manifest_path, {"version": 1, "entries": {}})
    previous_entries = previous.get("entries", {}) if isinstance(previous, dict) else {}
    current_entries: dict[str, dict] = {}

    source_files = collect_source_files(root)
    for relative_path, source_file in source_files.items():
        destination_file = destination_root / relative_path
        previous_entry = previous_entries.get(relative_path, {})
        try:
            source_stat = source_file.stat()
            source_hash = sha256_file(source_file)
            entry = {
                "sha256": source_hash,
                "size": source_stat.st_size,
                "mtime_ns": source_stat.st_mtime_ns,
            }
            current_entries[relative_path] = entry
            if previous_entry.get("sha256") == source_hash and destination_file.exists():
                result.skipped += 1
                continue
            atomic_copy(source_file, destination_file)
            result.copied += 1
        except (FileNotFoundError, OSError):
            if previous_entry:
                current_entries[relative_path] = previous_entry
            result.skipped += 1

    for relative_path, previous_entry in previous_entries.items():
        if relative_path in current_entries:
            continue
        if not root.delete_removed:
            current_entries[relative_path] = previous_entry
            continue
        if root.is_protected(relative_path):
            current_entries[relative_path] = previous_entry
            result.skipped += 1
            continue
        existing = destination_root / relative_path
        try:
            if existing.exists():
                existing.unlink()
                result.deleted += 1
        except OSError:
            current_entries[relative_path] = previous_entry
            result.failed += 1

    atomic_write_json(
        manifest_path,
        {
            "version": 1,
            "source": root.source.as_posix(),
            "destination": root.destination.as_posix(),
            "entries": current_entries,
        },
    )

    return result

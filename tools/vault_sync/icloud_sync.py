from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from tools.vault_sync.config import ICloudFolderConfig, ICloudSyncConfig, VaultConfig
from tools.vault_sync.document_converter import ConversionError, convert_document
from tools.vault_sync.io_utils import atomic_copy, atomic_write_text, resolve_vault_path, sha256_file
from tools.vault_sync.markdown import markdown_title, render_imported_markdown


@dataclass
class ICloudSyncResult:
    copied: int = 0
    converted: int = 0
    skipped: int = 0
    failed: int = 0
    warnings: list[str] = field(default_factory=list)


def sync_icloud(vault: VaultConfig, config: ICloudSyncConfig) -> ICloudSyncResult:
    result = ICloudSyncResult()
    if not config.enabled:
        return result

    for folder in config.folders:
        sync_icloud_folder(vault, config.root, folder, result)
    return result


def sync_icloud_folder(
    vault: VaultConfig,
    icloud_root: Path,
    folder: ICloudFolderConfig,
    result: ICloudSyncResult,
) -> None:
    source_root = icloud_root / folder.source
    if not source_root.exists():
        return

    raw_root = resolve_vault_path(vault.root, folder.raw_destination)
    converted_root = resolve_vault_path(vault.root, folder.converted_destination)
    raw_root.mkdir(parents=True, exist_ok=True)
    converted_root.mkdir(parents=True, exist_ok=True)

    raw_exts = set()
    if folder.include_markdown:
        raw_exts.add(".md")
    if folder.include_text:
        raw_exts.add(".txt")

    for path in source_root.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(source_root)
        suffix = path.suffix.lower()
        if suffix in raw_exts:
            destination = raw_root / rel
            destination.parent.mkdir(parents=True, exist_ok=True)
            try:
                source_hash = sha256_file(path)
                if destination.exists() and sha256_file(destination) == source_hash:
                    result.skipped += 1
                else:
                    atomic_copy(path, destination)
                    result.copied += 1
            except (FileNotFoundError, OSError) as error:
                result.failed += 1
                result.warnings.append(f"{rel.as_posix()}: {error}")
            continue

        out = converted_root / rel.with_name(f"{rel.name}.md")
        out.parent.mkdir(parents=True, exist_ok=True)
        source_hash = sha256_file(path)
        if out.exists() and f'memento_source_sha256: "{source_hash}"' in out.read_text(
            encoding="utf-8", errors="replace"
        ):
            result.skipped += 1
            continue

        enabled = (
            (suffix == ".doc" and folder.convert_doc)
            or (suffix == ".docx" and folder.convert_docx)
            or (suffix in {".ppt", ".pptx"} and folder.convert_pptx)
            or (suffix == ".pdf" and folder.convert_pdf)
        )
        if not enabled:
            continue
        try:
            conversion = convert_document(path)
            source_label = f"iCloud/{folder.source.as_posix()}/{rel.as_posix()}"
            rendered = render_imported_markdown(
                title=markdown_title(path, conversion.markdown),
                body=conversion.markdown,
                source=source_label,
                source_hash=source_hash,
                source_type=conversion.source_type,
                converter=conversion.converter,
                tags=["icloud", "imported", conversion.source_type],
                warnings=conversion.warnings,
            )
            atomic_write_text(out, rendered)
            result.converted += 1
            result.warnings.extend(f"{rel.as_posix()}: {warning}" for warning in conversion.warnings)
        except (ConversionError, OSError, UnicodeError) as error:
            result.failed += 1
            result.warnings.append(f"{rel.as_posix()}: {error}")

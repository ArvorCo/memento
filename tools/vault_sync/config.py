from __future__ import annotations

import fnmatch
import os
from dataclasses import dataclass
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - exercised on Python 3.10 hosts
    import tomli as tomllib


def expand_path(value: str) -> Path:
    return Path(os.path.expandvars(os.path.expanduser(value))).resolve()


@dataclass
class VaultConfig:
    root: Path
    state_dir: Path


@dataclass
class MarkdownSyncRoot:
    name: str
    source: Path
    destination: Path
    include_extensions: list[str]
    exclude_dirs: set[str]
    protected_globs: list[str]
    manifest: Path | None = None
    delete_removed: bool = True

    def is_protected(self, relative_path: str) -> bool:
        for pattern in self.protected_globs:
            if fnmatch.fnmatch(relative_path, pattern):
                return True
            if pattern.startswith("**/") and fnmatch.fnmatch(relative_path, pattern[3:]):
                return True
        return False


@dataclass
class LinkingConfig:
    enabled: bool
    default_project_prefix: str
    project_aliases: dict[str, str]
    hub_filename: str = "_memento_hub.md"
    root_hub: str = "_memento.md"
    tag_hubs: bool = True
    min_tag_documents: int = 2
    inject_navigation: bool = True
    exclude_dirs: set[str] | None = None

    def resolve_project_link(self, project_name: str) -> str | None:
        if not self.enabled or not project_name or project_name == "unknown":
            return None
        lowered = project_name.lower()
        for alias, target in self.project_aliases.items():
            if alias in lowered:
                return target
        return f"{self.default_project_prefix}/{project_name}"


@dataclass
class SessionImportConfig:
    enabled: bool
    source: Path
    destination: Path
    manifest: Path
    label: str
    source_tag: str
    file_glob: str
    exclude_path_fragments: list[str]


@dataclass
class DocumentImportConfig:
    name: str
    enabled: bool
    source: Path
    destination: Path
    manifest: Path
    include_extensions: set[str]
    exclude_dirs: set[str]
    preserve_raw: bool
    raw_destination: Path | None
    delete_removed: bool
    tags: list[str]
    max_file_bytes: int


@dataclass
class DatabaseImportConfig:
    name: str
    enabled: bool
    driver: str
    database: str | None
    dsn_env: str | None
    query: str
    destination: Path
    manifest: Path
    id_column: str
    title_column: str | None
    content_columns: list[str]
    metadata_columns: list[str]
    updated_at_column: str | None
    tags: list[str]
    delete_removed: bool


@dataclass
class SyncConfig:
    vault: VaultConfig
    markdown_roots: list[MarkdownSyncRoot]
    linking: LinkingConfig
    session_imports: dict[str, SessionImportConfig]
    document_imports: list[DocumentImportConfig]
    database_imports: list[DatabaseImportConfig]
    icloud: ICloudSyncConfig | None
    apple_notes: AppleNotesConfig | None
    whatsapp: WhatsAppImportConfig | None


@dataclass
class ICloudFolderConfig:
    name: str
    source: Path
    raw_destination: Path
    converted_destination: Path
    include_markdown: bool
    include_text: bool
    convert_doc: bool
    convert_docx: bool
    convert_pptx: bool
    convert_pdf: bool


@dataclass
class ICloudSyncConfig:
    enabled: bool
    root: Path
    folders: list[ICloudFolderConfig]


@dataclass
class AppleNotesConfig:
    enabled: bool
    destination: Path
    include_index: bool


@dataclass
class WhatsAppCategoryRule:
    name: str
    destination: Path
    matches: list[str]


@dataclass
class WhatsAppImportConfig:
    enabled: bool
    source: Path
    destination: Path
    manifest: Path
    category_rules: list[WhatsAppCategoryRule]
    default_category: Path


def load_config(config_path: str | Path | None = None) -> SyncConfig:
    path = (
        Path(config_path)
        if config_path
        else Path(
            os.getenv(
                "MEMENTO_VAULT_SYNC_CONFIG",
                Path(__file__).with_name("config.example.toml"),
            )
        )
    )
    data = tomllib.loads(path.read_text(encoding="utf-8"))

    vault_root = expand_path(data["vault"]["root"])
    state_dir = expand_path(data["vault"]["state_dir"])
    vault = VaultConfig(root=vault_root, state_dir=state_dir)

    markdown_roots = []
    for entry in data.get("markdown_sync", {}).get("roots", []):
        markdown_roots.append(
            MarkdownSyncRoot(
                name=entry["name"],
                source=expand_path(entry["source"]),
                destination=_vault_relative(entry["destination"], "markdown destination"),
                include_extensions=[
                    _normalize_extension(value) for value in entry.get("include_extensions", [".md"])
                ],
                exclude_dirs=set(entry.get("exclude_dirs", [])),
                protected_globs=list(entry.get("protected_globs", [])),
                manifest=_manifest_path(state_dir, entry.get("manifest", f"markdown-{entry['name']}.json")),
                delete_removed=bool(entry.get("delete_removed", True)),
            )
        )

    linking_data = data.get("linking", {})
    linking = LinkingConfig(
        enabled=bool(linking_data.get("enabled", False)),
        default_project_prefix=linking_data.get("default_project_prefix", "projects"),
        project_aliases=dict(linking_data.get("project_aliases", {})),
        hub_filename=_hub_filename(linking_data.get("hub_filename", "_memento_hub.md")),
        root_hub=_vault_relative(linking_data.get("root_hub", "_memento.md"), "root hub").as_posix(),
        tag_hubs=bool(linking_data.get("tag_hubs", True)),
        min_tag_documents=max(1, int(linking_data.get("min_tag_documents", 2))),
        inject_navigation=bool(linking_data.get("inject_navigation", True)),
        exclude_dirs=set(linking_data.get("exclude_dirs", [".git", ".obsidian", ".trash"])),
    )

    session_imports = {}
    for name, entry in data.get("session_import", {}).items():
        manifest_value = entry["manifest"]
        manifest_path = Path(manifest_value)
        if not manifest_path.is_absolute():
            manifest_path = state_dir / manifest_path
        session_imports[name] = SessionImportConfig(
            enabled=bool(entry.get("enabled", True)),
            source=expand_path(entry["source"]),
            destination=_vault_relative(entry["destination"], "session destination"),
            manifest=manifest_path.resolve(),
            label=entry.get("label", name.title()),
            source_tag=entry.get("source_tag", name),
            file_glob=entry.get("file_glob", "*.jsonl"),
            exclude_path_fragments=list(entry.get("exclude_path_fragments", [])),
        )

    document_imports = []
    for entry in data.get("document_import", {}).get("sources", []):
        manifest_path = _manifest_path(state_dir, entry.get("manifest", f"documents-{entry['name']}.json"))
        raw_destination = entry.get("raw_destination")
        document_imports.append(
            DocumentImportConfig(
                name=entry["name"],
                enabled=bool(entry.get("enabled", True)),
                source=expand_path(entry["source"]),
                destination=_vault_relative(entry["destination"], "document destination"),
                manifest=manifest_path,
                include_extensions={
                    _normalize_extension(value)
                    for value in entry.get(
                        "include_extensions",
                        [
                            ".md",
                            ".txt",
                            ".pdf",
                            ".doc",
                            ".docx",
                            ".odt",
                            ".rtf",
                            ".pptx",
                            ".xlsx",
                            ".html",
                            ".htm",
                            ".csv",
                            ".json",
                            ".ipynb",
                        ],
                    )
                },
                exclude_dirs=set(entry.get("exclude_dirs", [])),
                preserve_raw=bool(entry.get("preserve_raw", False)),
                raw_destination=(
                    _vault_relative(raw_destination, "raw document destination") if raw_destination else None
                ),
                delete_removed=bool(entry.get("delete_removed", True)),
                tags=list(entry.get("tags", ["documents", "imported"])),
                max_file_bytes=int(entry.get("max_file_bytes", 100 * 1024 * 1024)),
            )
        )

    database_imports = []
    for entry in data.get("database_import", {}).get("sources", []):
        manifest_path = _manifest_path(state_dir, entry.get("manifest", f"database-{entry['name']}.json"))
        database_imports.append(
            DatabaseImportConfig(
                name=entry["name"],
                enabled=bool(entry.get("enabled", False)),
                driver=str(entry.get("driver", "sqlite")).lower(),
                database=(str(entry["database"]) if entry.get("database") else None),
                dsn_env=(str(entry["dsn_env"]) if entry.get("dsn_env") else None),
                query=str(entry["query"]),
                destination=_vault_relative(entry["destination"], "database destination"),
                manifest=manifest_path,
                id_column=str(entry["id_column"]),
                title_column=(str(entry["title_column"]) if entry.get("title_column") else None),
                content_columns=[str(value) for value in entry.get("content_columns", [])],
                metadata_columns=[str(value) for value in entry.get("metadata_columns", [])],
                updated_at_column=(
                    str(entry["updated_at_column"]) if entry.get("updated_at_column") else None
                ),
                tags=list(entry.get("tags", ["database", "imported"])),
                delete_removed=bool(entry.get("delete_removed", True)),
            )
        )

    icloud_data = data.get("icloud_sync")
    icloud = None
    if icloud_data and (bool(icloud_data.get("enabled", False)) or "root" in icloud_data):
        folders = []
        root_path = expand_path(icloud_data["root"])
        for entry in icloud_data.get("folders", []):
            folders.append(
                ICloudFolderConfig(
                    name=entry["name"],
                    source=_vault_relative(entry["source"], "iCloud source"),
                    raw_destination=_vault_relative(entry["raw_destination"], "iCloud raw destination"),
                    converted_destination=_vault_relative(
                        entry["converted_destination"], "iCloud converted destination"
                    ),
                    include_markdown=bool(entry.get("include_markdown", True)),
                    include_text=bool(entry.get("include_text", True)),
                    convert_doc=bool(entry.get("convert_doc", True)),
                    convert_docx=bool(entry.get("convert_docx", True)),
                    convert_pptx=bool(entry.get("convert_pptx", False)),
                    convert_pdf=bool(entry.get("convert_pdf", True)),
                )
            )
        icloud = ICloudSyncConfig(
            enabled=bool(icloud_data.get("enabled", False)),
            root=root_path,
            folders=folders,
        )

    apple_notes_data = data.get("apple_notes")
    apple_notes = None
    if apple_notes_data and (
        bool(apple_notes_data.get("enabled", False)) or "destination" in apple_notes_data
    ):
        apple_notes = AppleNotesConfig(
            enabled=bool(apple_notes_data.get("enabled", False)),
            destination=_vault_relative(apple_notes_data["destination"], "Apple Notes destination"),
            include_index=bool(apple_notes_data.get("include_index", True)),
        )

    whatsapp_data = data.get("whatsapp_import")
    whatsapp = None
    if whatsapp_data and (
        bool(whatsapp_data.get("enabled", False))
        or all(key in whatsapp_data for key in ("source", "destination", "manifest"))
    ):
        manifest_value = whatsapp_data["manifest"]
        manifest_path = Path(manifest_value)
        if not manifest_path.is_absolute():
            manifest_path = state_dir / manifest_path
        category_rules = []
        for entry in whatsapp_data.get("category_rules", []):
            category_rules.append(
                WhatsAppCategoryRule(
                    name=entry["name"],
                    destination=_vault_relative(entry["destination"], "WhatsApp category destination"),
                    matches=list(entry.get("matches", [])),
                )
            )
        whatsapp = WhatsAppImportConfig(
            enabled=bool(whatsapp_data.get("enabled", False)),
            source=expand_path(whatsapp_data["source"]),
            destination=_vault_relative(whatsapp_data["destination"], "WhatsApp destination"),
            manifest=manifest_path.resolve(),
            category_rules=category_rules,
            default_category=_vault_relative(
                whatsapp_data.get("default_category", "outros"), "WhatsApp default category"
            ),
        )

    return SyncConfig(
        vault=vault,
        markdown_roots=markdown_roots,
        linking=linking,
        session_imports=session_imports,
        document_imports=document_imports,
        database_imports=database_imports,
        icloud=icloud,
        apple_notes=apple_notes,
        whatsapp=whatsapp,
    )


def _manifest_path(state_dir: Path, value: str) -> Path:
    path = Path(value)
    if not path.is_absolute():
        path = state_dir / path
    return path.resolve()


def _vault_relative(value: str, label: str) -> Path:
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"{label} must stay inside the vault: {value}")
    return path


def _normalize_extension(value: str) -> str:
    normalized = value.strip().lower()
    if not normalized:
        raise ValueError("document include_extensions cannot contain an empty value")
    return normalized if normalized.startswith(".") else f".{normalized}"


def _hub_filename(value: str) -> str:
    path = Path(value)
    if path.name != value or value in {"", ".", ".."}:
        raise ValueError(f"hub_filename must be a file name, not a path: {value}")
    return value

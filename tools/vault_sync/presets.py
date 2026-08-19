from __future__ import annotations

import sys
from pathlib import Path

PRESET_TEMPLATES = {
    "mac": """[vault]
root = "{vault_root}"
state_dir = "{state_dir}"

[[markdown_sync.roots]]
name = "documents"
source = "${{HOME}}/Documents"
destination = "documents"
include_extensions = [".md", ".txt"]
exclude_dirs = [".git", "node_modules", "__pycache__", ".venv", "venv", "dist", "build"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md", "**/*Hub*.md", "**/* Hub.md"]

[[markdown_sync.roots]]
name = "desktop"
source = "${{HOME}}/Desktop"
destination = "desktop"
include_extensions = [".md", ".txt"]
exclude_dirs = [".git", "node_modules", "__pycache__", ".venv", "venv"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md"]

[[markdown_sync.roots]]
name = "workspace"
source = "${{HOME}}/Developer"
destination = "projects"
include_extensions = [".md"]
exclude_dirs = [".git", "node_modules", "Pods", ".next", ".turbo", "__pycache__", ".venv", "venv", "dist", "build", "DerivedData", ".codex", ".factory", ".claude", ".gemini"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md", "**/*Hub*.md", "**/* Hub.md"]

[linking]
enabled = true
default_project_prefix = "projects"
hub_filename = "_memento_hub.md"
root_hub = "_memento.md"
tag_hubs = true
min_tag_documents = 2
inject_navigation = true
exclude_dirs = [".git", ".obsidian", ".trash"]

[session_import.codex]
enabled = true
source = "${{HOME}}/.codex/sessions"
destination = "converted/codex"
manifest = "codex_manifest.json"
label = "Codex"
source_tag = "codex"
file_glob = "*.jsonl"

[session_import.droid]
enabled = true
source = "${{HOME}}/.factory/sessions"
destination = "converted/droid"
manifest = "droid_manifest.json"
label = "Droid"
source_tag = "droid"
file_glob = "*.jsonl"

[session_import.claude]
enabled = true
source = "${{HOME}}/.claude/projects"
destination = "converted/claude"
manifest = "claude_manifest.json"
label = "Claude"
source_tag = "claude"
file_glob = "*.jsonl"
exclude_path_fragments = ["subagents"]

[session_import.chatgpt]
enabled = false
source = "${{HOME}}/Downloads/chatgpt-export"
destination = "converted/chatgpt"
manifest = "chatgpt_manifest.json"
label = "ChatGPT"
source_tag = "chatgpt"
file_glob = "conversations*.json"

[icloud_sync]
enabled = false
root = "${{HOME}}/Library/Mobile Documents/com~apple~CloudDocs"

[[icloud_sync.folders]]
name = "documents"
source = "Documents"
raw_destination = "raw/icloud/documents"
converted_destination = "converted/icloud/documents"
include_markdown = true
include_text = true
convert_doc = true
convert_docx = true
convert_pptx = false
convert_pdf = true

[apple_notes]
enabled = false
destination = "converted/apple-notes"
include_index = true

[whatsapp_import]
enabled = false
source = "${{HOME}}/Downloads"
destination = "whatsapp"
manifest = "whatsapp_manifest.json"
default_category = "outros"
""",
    "linux": """[vault]
root = "{vault_root}"
state_dir = "{state_dir}"

[[markdown_sync.roots]]
name = "documents"
source = "${{HOME}}/Documents"
destination = "documents"
include_extensions = [".md", ".txt"]
exclude_dirs = [".git", "node_modules", "__pycache__", ".venv", "venv", "dist", "build"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md", "**/*Hub*.md", "**/* Hub.md"]

[[markdown_sync.roots]]
name = "workspace"
source = "${{HOME}}/Projects"
destination = "projects"
include_extensions = [".md"]
exclude_dirs = [".git", "node_modules", "__pycache__", ".venv", "venv", "dist", "build", ".codex", ".factory", ".claude", ".gemini"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md", "**/*Hub*.md", "**/* Hub.md"]

[linking]
enabled = true
default_project_prefix = "projects"
hub_filename = "_memento_hub.md"
root_hub = "_memento.md"
tag_hubs = true
min_tag_documents = 2
inject_navigation = true
exclude_dirs = [".git", ".obsidian", ".trash"]

[session_import.codex]
enabled = true
source = "${{HOME}}/.codex/sessions"
destination = "converted/codex"
manifest = "codex_manifest.json"
label = "Codex"
source_tag = "codex"
file_glob = "*.jsonl"

[session_import.droid]
enabled = true
source = "${{HOME}}/.factory/sessions"
destination = "converted/droid"
manifest = "droid_manifest.json"
label = "Droid"
source_tag = "droid"
file_glob = "*.jsonl"

[session_import.claude]
enabled = true
source = "${{HOME}}/.claude/projects"
destination = "converted/claude"
manifest = "claude_manifest.json"
label = "Claude"
source_tag = "claude"
file_glob = "*.jsonl"
exclude_path_fragments = ["subagents"]

[session_import.chatgpt]
enabled = false
source = "${{HOME}}/Downloads/chatgpt-export"
destination = "converted/chatgpt"
manifest = "chatgpt_manifest.json"
label = "ChatGPT"
source_tag = "chatgpt"
file_glob = "conversations*.json"

[whatsapp_import]
enabled = false
source = "${{HOME}}/Downloads"
destination = "whatsapp"
manifest = "whatsapp_manifest.json"
default_category = "outros"
""",
    "windows": """[vault]
root = "{vault_root}"
state_dir = "{state_dir}"

[[markdown_sync.roots]]
name = "documents"
source = "${{USERPROFILE}}/Documents"
destination = "documents"
include_extensions = [".md", ".txt"]
exclude_dirs = [".git", "node_modules", "__pycache__", ".venv", "venv", "dist", "build"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md", "**/*Hub*.md", "**/* Hub.md"]

[[markdown_sync.roots]]
name = "desktop"
source = "${{USERPROFILE}}/Desktop"
destination = "desktop"
include_extensions = [".md", ".txt"]
exclude_dirs = [".git", "node_modules", "__pycache__", ".venv", "venv"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md"]

[[markdown_sync.roots]]
name = "workspace"
source = "${{USERPROFILE}}/source/repos"
destination = "projects"
include_extensions = [".md"]
exclude_dirs = [".git", "node_modules", "__pycache__", ".venv", "venv", "dist", "build", ".codex", ".factory", ".claude", ".gemini"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md", "**/*Hub*.md", "**/* Hub.md"]

[linking]
enabled = true
default_project_prefix = "projects"
hub_filename = "_memento_hub.md"
root_hub = "_memento.md"
tag_hubs = true
min_tag_documents = 2
inject_navigation = true
exclude_dirs = [".git", ".obsidian", ".trash"]

[session_import.codex]
enabled = true
source = "${{USERPROFILE}}/.codex/sessions"
destination = "converted/codex"
manifest = "codex_manifest.json"
label = "Codex"
source_tag = "codex"
file_glob = "*.jsonl"

[session_import.droid]
enabled = true
source = "${{USERPROFILE}}/.factory/sessions"
destination = "converted/droid"
manifest = "droid_manifest.json"
label = "Droid"
source_tag = "droid"
file_glob = "*.jsonl"

[session_import.claude]
enabled = true
source = "${{USERPROFILE}}/.claude/projects"
destination = "converted/claude"
manifest = "claude_manifest.json"
label = "Claude"
source_tag = "claude"
file_glob = "*.jsonl"
exclude_path_fragments = ["subagents"]

[session_import.chatgpt]
enabled = false
source = "${{USERPROFILE}}/Downloads/chatgpt-export"
destination = "converted/chatgpt"
manifest = "chatgpt_manifest.json"
label = "ChatGPT"
source_tag = "chatgpt"
file_glob = "conversations*.json"

[whatsapp_import]
enabled = false
source = "${{USERPROFILE}}/Downloads"
destination = "whatsapp"
manifest = "whatsapp_manifest.json"
default_category = "outros"
""",
}

OPTIONAL_IMPORTS = """

[[document_import.sources]]
name = "personal-documents"
enabled = false
source = "__HOME__/Documents"
destination = "converted/documents"
manifest = "documents-personal.json"
include_extensions = [".pdf", ".doc", ".docx", ".odt", ".rtf", ".pptx", ".xlsx", ".html", ".csv", ".json", ".ipynb"]
exclude_dirs = [".git", ".obsidian", "node_modules", ".venv", "venv"]
preserve_raw = false
delete_removed = true
tags = ["documents", "imported"]
max_file_bytes = 104857600

[[database_import.sources]]
name = "example-sqlite"
enabled = false
driver = "sqlite"
database = "__HOME__/data/app.db"
query = "SELECT id, title, body, updated_at FROM notes"
destination = "databases/example"
manifest = "database-example.json"
id_column = "id"
title_column = "title"
content_columns = ["body"]
metadata_columns = ["updated_at"]
updated_at_column = "updated_at"
tags = ["database", "notes"]
delete_removed = true
"""


def detect_preset_name() -> str:
    if sys.platform.startswith("darwin"):
        return "mac"
    if sys.platform.startswith("win"):
        return "windows"
    return "linux"


def default_vault_root(preset_name: str) -> str:
    if preset_name == "windows":
        return "${USERPROFILE}/MementoVault"
    return "${HOME}/MementoVault"


def default_state_dir(preset_name: str) -> str:
    if preset_name == "windows":
        return "${USERPROFILE}/.memento/sync"
    return "${HOME}/.memento/sync"


def render_preset(
    preset_name: str,
    *,
    vault_root: str | None = None,
    state_dir: str | None = None,
) -> str:
    template = PRESET_TEMPLATES[preset_name]
    rendered = template.format(
        vault_root=_toml_string_content(vault_root or default_vault_root(preset_name)),
        state_dir=_toml_string_content(state_dir or default_state_dir(preset_name)),
    )
    home = "${USERPROFILE}" if preset_name == "windows" else "${HOME}"
    return rendered + OPTIONAL_IMPORTS.replace("__HOME__", home)


def _toml_string_content(value: str) -> str:
    return (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\b", "\\b")
        .replace("\t", "\\t")
        .replace("\n", "\\n")
        .replace("\f", "\\f")
        .replace("\r", "\\r")
    )


def write_preset(
    output_path: Path,
    *,
    preset_name: str,
    vault_root: str | None = None,
    state_dir: str | None = None,
) -> Path:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        render_preset(preset_name, vault_root=vault_root, state_dir=state_dir),
        encoding="utf-8",
    )
    return output_path

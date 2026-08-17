from __future__ import annotations

import re
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from tools.vault_sync.config import AppleNotesConfig, VaultConfig


@dataclass
class AppleNotesResult:
    exported: int = 0
    converted: int = 0
    failed: int = 0
    index_path: Path | None = None


def safe_name(value: str, maxlen: int = 120) -> str:
    return re.sub(r'[/:*?"<>|\\]', "_", value).strip()[:maxlen] or "Untitled"


def html_to_markdown(html: str) -> str:
    text = html
    replacements = [
        (r"<br\s*/?>", "\n"),
        (r"</p>", "\n"),
        (r"<p[^>]*>", ""),
        (r"</div>", "\n"),
        (r"<div[^>]*>", ""),
        (r"<h1[^>]*>(.*?)</h1>", r"# \1"),
        (r"<h2[^>]*>(.*?)</h2>", r"## \1"),
        (r"<h3[^>]*>(.*?)</h3>", r"### \1"),
        (r"<b[^>]*>(.*?)</b>", r"**\1**"),
        (r"<strong[^>]*>(.*?)</strong>", r"**\1**"),
        (r"<i[^>]*>(.*?)</i>", r"*\1*"),
        (r"<em[^>]*>(.*?)</em>", r"*\1*"),
        (r"<li[^>]*>(.*?)</li>", r"- \1"),
        (r'<a href="([^"]+)"[^>]*>(.*?)</a>', r"[\2](\1)"),
        (r"<[^>]+>", ""),
    ]
    for pattern, repl in replacements:
        text = re.sub(pattern, repl, text, flags=re.DOTALL)
    text = (
        text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&quot;", '"')
        .replace("\u00a0", " ")
    )
    lines = [line.rstrip() for line in text.splitlines()]
    out = []
    prev_blank = False
    for line in lines:
        if not line.strip():
            if not prev_blank:
                out.append("")
            prev_blank = True
            continue
        out.append(line)
        prev_blank = False
    return "\n".join(out).strip()


def convert_exported_notes(raw_dir: Path, destination_root: Path, include_index: bool = True) -> AppleNotesResult:
    result = AppleNotesResult()
    destination_root.mkdir(parents=True, exist_ok=True)

    for path in sorted(raw_dir.glob("*.txt"), key=lambda item: item.name):
        result.exported += 1
        try:
            content = path.read_text(encoding="utf-8", errors="replace")
            parts = content.split("\n", 2)
            if len(parts) < 2:
                result.failed += 1
                continue
            folder = parts[0].strip() or "Notes"
            title = parts[1].strip() or "Untitled"
            body = parts[2] if len(parts) > 2 else ""
            folder_dir = destination_root / safe_name(folder)
            folder_dir.mkdir(parents=True, exist_ok=True)
            markdown_body = (
                "> This note contains attachments and could not be exported as text.\n"
                if body.strip() == "__HAS_ATTACHMENTS__"
                else html_to_markdown(body)
            )
            out = folder_dir / f"{safe_name(title)}.md"
            if out.exists():
                stem = out.stem
                index = 2
                while (folder_dir / f"{stem}_{index}.md").exists():
                    index += 1
                out = folder_dir / f"{stem}_{index}.md"
            out.write_text(
                "\n".join(
                    [
                        "---",
                        f'title: "{title.replace(chr(34), chr(39))}"',
                        f'folder: "{folder.replace(chr(34), chr(39))}"',
                        "tags: [apple-notes, imported]",
                        "---",
                        "",
                        f"# {title}",
                        "",
                        markdown_body,
                        "",
                    ]
                ),
                encoding="utf-8",
            )
            result.converted += 1
        except Exception:
            result.failed += 1

    if include_index:
        result.index_path = write_notes_index(destination_root)
    return result


def write_notes_index(destination_root: Path) -> Path:
    index_path = destination_root / "_INDEX.md"
    lines = [
        "---",
        'title: "Apple Notes Index"',
        "tags: [apple-notes, index]",
        "---",
        "",
        "# Apple Notes",
        "",
    ]
    for folder_dir in sorted(path for path in destination_root.iterdir() if path.is_dir()):
        notes = sorted(folder_dir.glob("*.md"))
        lines.append(f"## {folder_dir.name}")
        lines.append("")
        for note in notes:
            rel = note.relative_to(destination_root).with_suffix("")
            lines.append(f"- [[{rel.as_posix()}|{note.stem}]]")
        lines.append("")
    index_path.write_text("\n".join(lines), encoding="utf-8")
    return index_path


def export_apple_notes(vault: VaultConfig, config: AppleNotesConfig) -> AppleNotesResult:
    if not config.enabled:
        return AppleNotesResult()
    if shutil.which("osascript") is None:
        return AppleNotesResult(failed=1)
    destination_root = vault.root / config.destination

    with tempfile.TemporaryDirectory() as tempdir:
        raw_dir = Path(tempdir)
        script = f"""
set tmpDir to "{raw_dir}"
set deletedNames to {{"Recently Deleted", "Apagadas recentemente", "Apagados recentemente", "Excluídos recentemente"}}
set noteIndex to 0
tell application "Notes"
    repeat with eachFolder in folders
        set folderName to name of eachFolder
        if folderName is not in deletedNames then
            repeat with eachNote in notes of eachFolder
                try
                    set noteIndex to noteIndex + 1
                    set noteTitle to name of eachNote
                    set noteBody to body of eachNote
                    set outPath to tmpDir & "/" & (noteIndex as string) & ".txt"
                    set fileContent to folderName & "\\n" & noteTitle & "\\n" & noteBody
                    do shell script "printf '%s' " & quoted form of fileContent & " > " & quoted form of outPath
                on error
                    set noteIndex to noteIndex + 1
                    set outPath to tmpDir & "/" & (noteIndex as string) & ".txt"
                    do shell script "printf '%s' " & quoted form of (folderName & "\\n" & (name of eachNote) & "\\n__HAS_ATTACHMENTS__") & " > " & quoted form of outPath
                end try
            end repeat
        end if
    end repeat
end tell
"""
        result = subprocess.run(["osascript", "-e", script], capture_output=True, text=True, check=False)
        if result.returncode != 0:
            return AppleNotesResult(failed=1)
        return convert_exported_notes(raw_dir, destination_root, include_index=config.include_index)

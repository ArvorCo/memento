#!/usr/bin/env python3
"""Validate repository Markdown without network access or third-party packages."""

from __future__ import annotations

import re
import sys
import unicodedata
from collections import Counter
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
EXCLUDED_PARTS = {
    ".git",
    ".next",
    ".venv",
    "node_modules",
    "target",
    "venv",
}
PUBLIC_ROOT_DOCS = {
    "AGENTS.md",
    "AGENT_INSTALL.md",
    "ARCHITECTURE.md",
    "CHANGELOG.md",
    "CODE_OF_CONDUCT.md",
    "CONTRIBUTING.md",
    "README.md",
    "ROADMAP.md",
    "SECURITY.md",
    "SUPPORT.md",
    "VISION.md",
}
LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
HEADING_RE = re.compile(r"^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$")
FENCE_RE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
TABLE_SEPARATOR_RE = re.compile(r"^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$")
FORBIDDEN_PUBLIC_PATTERNS = {
    re.compile(r"/Users/(?!<name>)[A-Za-z0-9._-]+"): "personal absolute path",
}


def public_markdown_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*.md"):
        relative = path.relative_to(ROOT)
        if any(part in EXCLUDED_PARTS for part in relative.parts):
            continue
        if relative.parts[0] == "docs" or relative.as_posix() in PUBLIC_ROOT_DOCS:
            files.append(path)
    return sorted(files)


def github_slug(value: str) -> str:
    value = re.sub(r"<[^>]+>", "", value)
    value = re.sub(r"!?\[([^\]]*)\]\([^)]+\)", r"\1", value)
    value = value.replace("`", "").strip().lower()
    value = "".join(
        character
        for character in value
        if not unicodedata.category(character).startswith(("P", "S")) or character in {"-", "_"}
    )
    value = re.sub(r"\s+", "-", value)
    return value


def anchors_for(path: Path) -> set[str]:
    counts: Counter[str] = Counter()
    anchors: set[str] = set()
    in_fence = False
    fence_marker = ""
    for line in path.read_text(encoding="utf-8").splitlines():
        fence = FENCE_RE.match(line)
        if fence:
            marker = fence.group(1)
            if not in_fence:
                in_fence = True
                fence_marker = marker[0]
            elif marker[0] == fence_marker:
                in_fence = False
            continue
        if in_fence:
            continue
        heading = HEADING_RE.match(line)
        if not heading:
            continue
        base = github_slug(heading.group(1))
        if not base:
            continue
        suffix = counts[base]
        counts[base] += 1
        anchors.add(base if suffix == 0 else f"{base}-{suffix}")
    return anchors


def split_link(raw: str) -> tuple[str, str]:
    raw = raw.strip()
    # Markdown permits an optional quoted title after a whitespace-separated URL.
    raw = raw[1 : raw.index(">")] if raw.startswith("<") and ">" in raw else re.split(r"\s+[\"']", raw, maxsplit=1)[0]
    parsed = urlsplit(raw)
    return unquote(parsed.path), unquote(parsed.fragment)


def validate_links(path: Path, errors: list[str]) -> None:
    in_fence = False
    fence_marker = ""
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fence = FENCE_RE.match(line)
        if fence:
            marker = fence.group(1)
            if not in_fence:
                in_fence = True
                fence_marker = marker[0]
            elif marker[0] == fence_marker:
                in_fence = False
            continue
        if in_fence:
            continue
        for match in LINK_RE.finditer(line):
            raw = match.group(1).strip()
            if not raw or raw.startswith(("http://", "https://", "mailto:", "app://")):
                continue
            target_text, fragment = split_link(raw)
            target = path if not target_text else (path.parent / target_text).resolve()
            try:
                target.relative_to(ROOT)
            except ValueError:
                errors.append(f"{path.relative_to(ROOT)}:{line_number}: link escapes repository: {raw}")
                continue
            if not target.exists():
                errors.append(f"{path.relative_to(ROOT)}:{line_number}: missing link target: {raw}")
                continue
            if fragment and target.is_file() and target.suffix.lower() == ".md" and fragment not in anchors_for(target):
                errors.append(
                    f"{path.relative_to(ROOT)}:{line_number}: missing Markdown anchor "
                    f"#{fragment} in {target.relative_to(ROOT)}"
                )


def validate_fences_and_mermaid(path: Path, errors: list[str]) -> None:
    lines = path.read_text(encoding="utf-8").splitlines()
    open_line: int | None = None
    fence_marker = ""
    fence_info = ""
    block: list[str] = []
    for line_number, line in enumerate(lines, 1):
        fence = FENCE_RE.match(line)
        if fence and open_line is None:
            open_line = line_number
            fence_marker = fence.group(1)[0]
            fence_info = fence.group(2).strip().split(maxsplit=1)[0].lower()
            block = []
            continue
        if fence and open_line is not None and fence.group(1)[0] == fence_marker:
            if fence_info == "mermaid":
                content = "\n".join(block)
                if "accTitle:" not in content:
                    errors.append(f"{path.relative_to(ROOT)}:{open_line}: Mermaid block lacks accTitle")
                if "accDescr:" not in content:
                    errors.append(f"{path.relative_to(ROOT)}:{open_line}: Mermaid block lacks accDescr")
            open_line = None
            fence_marker = ""
            fence_info = ""
            block = []
            continue
        if open_line is not None:
            block.append(line)
    if open_line is not None:
        errors.append(f"{path.relative_to(ROOT)}:{open_line}: unclosed code fence")


def table_column_count(line: str) -> int:
    unescaped = re.sub(r"\\\|", "", line.strip())
    pipes = unescaped.count("|")
    if unescaped.startswith("|") and unescaped.endswith("|"):
        return max(0, pipes - 1)
    return pipes + 1


def validate_tables(path: Path, errors: list[str]) -> None:
    lines = path.read_text(encoding="utf-8").splitlines()
    in_fence = False
    fence_marker = ""
    expected_columns: int | None = None
    for index, line in enumerate(lines):
        fence = FENCE_RE.match(line)
        if fence:
            marker = fence.group(1)
            if not in_fence:
                in_fence = True
                fence_marker = marker[0]
            elif marker[0] == fence_marker:
                in_fence = False
            expected_columns = None
            continue
        if in_fence:
            continue
        if TABLE_SEPARATOR_RE.match(line):
            expected_columns = table_column_count(line)
            if index == 0 or table_column_count(lines[index - 1]) != expected_columns:
                errors.append(
                    f"{path.relative_to(ROOT)}:{index + 1}: table header and separator have different column counts"
                )
            continue
        if expected_columns is not None:
            if not line.strip() or "|" not in line:
                expected_columns = None
                continue
            actual_columns = table_column_count(line)
            if actual_columns != expected_columns:
                errors.append(
                    f"{path.relative_to(ROOT)}:{index + 1}: table expects "
                    f"{expected_columns} columns, found {actual_columns}; escape cell pipes as \\|"
                )


def validate_public_hygiene(path: Path, errors: list[str]) -> None:
    relative = path.relative_to(ROOT)
    text = path.read_text(encoding="utf-8")
    line_count = text.count("\n") + (0 if text.endswith("\n") else 1)
    if line_count > 1_000:
        errors.append(f"{relative}: {line_count} lines exceeds the 1,000-line documentation limit")
    for pattern, description in FORBIDDEN_PUBLIC_PATTERNS.items():
        if match := pattern.search(text):
            line = text[: match.start()].count("\n") + 1
            errors.append(f"{relative}:{line}: contains {description}: {match.group()}")


def main() -> int:
    errors: list[str] = []
    files = public_markdown_files()
    for path in files:
        validate_links(path, errors)
        validate_fences_and_mermaid(path, errors)
        validate_tables(path, errors)
        validate_public_hygiene(path, errors)

    if errors:
        print("Documentation validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    mermaid_count = sum(path.read_text(encoding="utf-8").count("```mermaid") for path in files)
    print(f"Documentation OK: {len(files)} public Markdown files, {mermaid_count} accessible Mermaid diagrams.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

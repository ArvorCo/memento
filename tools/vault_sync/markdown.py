from __future__ import annotations

import re
from pathlib import Path


def yaml_scalar(value: object) -> str:
    text = str(value).replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ")
    return f'"{text}"'


def normalize_markdown(text: str) -> str:
    text = text.replace("\r\n", "\n").replace("\r", "\n").replace("\u00a0", " ")
    lines = [line.rstrip() for line in text.splitlines()]
    normalized: list[str] = []
    previous_blank = False
    for line in lines:
        is_blank = not line.strip()
        if is_blank and previous_blank:
            continue
        normalized.append(line)
        previous_blank = is_blank
    return "\n".join(normalized).strip() + "\n"


def markdown_title(path: Path, content: str | None = None) -> str:
    if content:
        heading = re.search(r"(?m)^#\s+(.+?)\s*$", content)
        if heading:
            return heading.group(1).strip()
    return path.stem.replace("_", " ").replace("-", " ").strip().title() or "Untitled"


def render_imported_markdown(
    *,
    title: str,
    body: str,
    source: str,
    source_hash: str,
    source_type: str,
    converter: str,
    tags: list[str],
    warnings: list[str] | None = None,
) -> str:
    unique_tags = list(dict.fromkeys(tag.strip() for tag in tags if tag.strip()))
    lines = [
        "---",
        f"title: {yaml_scalar(title)}",
        f"memento_source: {yaml_scalar(source)}",
        f"memento_source_sha256: {yaml_scalar(source_hash)}",
        f"memento_source_type: {yaml_scalar(source_type)}",
        f"memento_converter: {yaml_scalar(converter)}",
        f"tags: [{', '.join(yaml_scalar(tag) for tag in unique_tags)}]",
    ]
    if warnings:
        lines.append(f"memento_conversion_warnings: [{', '.join(yaml_scalar(value) for value in warnings)}]")
    lines.extend(["---", "", f"# {title}", "", body.strip(), ""])
    return normalize_markdown("\n".join(lines))


def augment_markdown_source(
    *,
    content: str,
    title: str,
    source: str,
    source_hash: str,
    source_type: str,
    converter: str,
    tags: list[str],
) -> str:
    normalized = normalize_markdown(content)
    provenance = [
        f"memento_source: {yaml_scalar(source)}",
        f"memento_source_sha256: {yaml_scalar(source_hash)}",
        f"memento_source_type: {yaml_scalar(source_type)}",
        f"memento_converter: {yaml_scalar(converter)}",
        f"memento_tags: [{', '.join(yaml_scalar(tag) for tag in tags if tag.strip())}]",
    ]
    if normalized.startswith("---\n"):
        closing = normalized.find("\n---\n", 4)
        if closing != -1:
            frontmatter = normalized[4:closing]
            body = normalized[closing + 5 :]
            kept = [
                line
                for line in frontmatter.splitlines()
                if not line.startswith("memento_source:")
                and not line.startswith("memento_source_sha256:")
                and not line.startswith("memento_source_type:")
                and not line.startswith("memento_converter:")
                and not line.startswith("memento_tags:")
            ]
            return normalize_markdown("\n".join(["---", *kept, *provenance, "---", "", body]))
    return render_imported_markdown(
        title=title,
        body=normalized,
        source=source,
        source_hash=source_hash,
        source_type=source_type,
        converter=converter,
        tags=tags,
    )

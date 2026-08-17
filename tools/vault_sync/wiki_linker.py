from __future__ import annotations

import re
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

from tools.vault_sync.config import LinkingConfig, VaultConfig
from tools.vault_sync.io_utils import atomic_write_text, resolve_vault_path, safe_filename
from tools.vault_sync.markdown import markdown_title, normalize_markdown, yaml_scalar

NAV_START = "<!-- memento:nav:start -->"
NAV_END = "<!-- memento:nav:end -->"
GENERATED_MARKER = "memento_generated: true"
GENERIC_TAGS = {"document", "documents", "imported", "memento", "note", "notes"}


@dataclass(frozen=True)
class WikiDocument:
    path: Path
    relative: Path
    title: str
    tags: tuple[str, ...]

    @property
    def target(self) -> str:
        return self.relative.with_suffix("").as_posix()


@dataclass
class WikiLinkResult:
    documents: int = 0
    directory_hubs: int = 0
    tag_hubs: int = 0
    navigation_updated: int = 0
    unchanged: int = 0
    failed: int = 0
    warnings: list[str] = field(default_factory=list)


def link_vault(vault: VaultConfig, config: LinkingConfig) -> WikiLinkResult:
    result = WikiLinkResult()
    if not config.enabled:
        return result
    root = vault.root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    documents = _discover_documents(root, config)
    result.documents = len(documents)
    by_directory: dict[Path, list[WikiDocument]] = defaultdict(list)
    by_tag: dict[str, list[WikiDocument]] = defaultdict(list)
    for document in documents:
        by_directory[document.relative.parent].append(document)
        for tag in document.tags:
            if tag.lower() not in GENERIC_TAGS:
                by_tag[tag].append(document)

    hub_targets = _directory_hub_targets(config, by_directory)
    for directory in sorted(hub_targets, key=lambda value: (len(value.parts), value.as_posix()), reverse=True):
        hub_relative = hub_targets[directory]
        children = sorted(
            (child, target)
            for child, target in hub_targets.items()
            if child.parent == directory and child != directory
        )
        content = _render_directory_hub(
            config=config,
            directory=directory,
            hub_relative=hub_relative,
            documents=sorted(by_directory.get(directory, []), key=lambda value: value.title.casefold()),
            child_hubs=children,
            tags=_directory_tags(by_directory.get(directory, []), by_tag, config.min_tag_documents),
        )
        if _write_generated(root, hub_relative, content, result):
            result.directory_hubs += 1

    eligible_tags = {
        tag: sorted(values, key=lambda value: (value.title.casefold(), value.target))
        for tag, values in by_tag.items()
        if config.tag_hubs and len(values) >= config.min_tag_documents
    }
    for tag, tagged_documents in sorted(eligible_tags.items(), key=lambda item: item[0].casefold()):
        tag_relative = _tag_hub_relative(tag)
        if _write_generated(
            root,
            tag_relative,
            _render_tag_hub(tag, tagged_documents, Path(config.root_hub)),
            result,
        ):
            result.tag_hubs += 1

    if config.inject_navigation:
        for document in documents:
            directory_hub = hub_targets.get(document.relative.parent)
            tag_targets = [
                (tag, _tag_hub_relative(tag))
                for tag in document.tags
                if tag in eligible_tags and tag.lower() not in GENERIC_TAGS
            ][:4]
            if _inject_navigation(document, directory_hub, tag_targets, result):
                result.navigation_updated += 1

    return result


def _discover_documents(root: Path, config: LinkingConfig) -> list[WikiDocument]:
    excluded = config.exclude_dirs or set()
    generated_names = {config.hub_filename, Path(config.root_hub).name}
    documents = []
    for path in sorted(root.rglob("*.md")):
        relative = path.relative_to(root)
        if any(part in excluded for part in relative.parts):
            continue
        if relative.parts and relative.parts[0] == "_memento":
            continue
        if path.name in generated_names:
            continue
        try:
            content = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if GENERATED_MARKER in content:
            continue
        documents.append(
            WikiDocument(
                path=path,
                relative=relative,
                title=_frontmatter_title(content) or markdown_title(path, content),
                tags=tuple(sorted(_extract_tags(content), key=str.casefold)),
            )
        )
    return documents


def _directory_hub_targets(
    config: LinkingConfig, by_directory: dict[Path, list[WikiDocument]]
) -> dict[Path, Path]:
    directories = set(by_directory)
    for directory in list(directories):
        parent = directory
        while parent != Path("."):
            parent = parent.parent
            directories.add(parent)
    directories.add(Path("."))
    targets = {}
    for directory in directories:
        target = Path(config.root_hub) if directory == Path(".") else directory / config.hub_filename
        if target.is_absolute() or ".." in target.parts:
            raise ValueError(f"hub path escapes vault: {target}")
        targets[directory] = target
    return targets


def _render_directory_hub(
    *,
    config: LinkingConfig,
    directory: Path,
    hub_relative: Path,
    documents: list[WikiDocument],
    child_hubs: list[tuple[Path, Path]],
    tags: list[str],
) -> str:
    label = "Memento" if directory == Path(".") else directory.name.replace("-", " ").replace("_", " ").title()
    lines = [
        "---",
        f"title: {yaml_scalar(f'{label} Hub')}",
        GENERATED_MARKER,
        f"memento_directory: {yaml_scalar(directory.as_posix())}",
        'tags: ["memento", "hub"]',
        "---",
        "",
        f"# {label}",
        "",
        "<!-- memento:hub:start -->",
    ]
    parent = directory.parent if directory != Path(".") else None
    if parent is not None:
        parent_target = Path(config.root_hub) if parent == Path(".") else parent / config.hub_filename
        lines.extend([f"> [[{parent_target.with_suffix('').as_posix()}|← Parent hub]]", ""])
    if child_hubs:
        lines.extend(["## Sections", ""])
        lines.extend(
            f"- [[{target.with_suffix('').as_posix()}|{child.name.replace('-', ' ').replace('_', ' ').title()}]]"
            for child, target in child_hubs
        )
        lines.append("")
    if documents:
        lines.extend(["## Documents", ""])
        lines.extend(f"- [[{document.target}|{document.title}]]" for document in documents)
        lines.append("")
    if tags:
        lines.extend(["## Topics", ""])
        lines.extend(f"- [[{_tag_hub_relative(tag).with_suffix('').as_posix()}|#{tag}]]" for tag in tags)
        lines.append("")
    lines.extend(["<!-- memento:hub:end -->", ""])
    return normalize_markdown("\n".join(lines))


def _render_tag_hub(tag: str, documents: list[WikiDocument], root_hub: Path) -> str:
    lines = [
        "---",
        f"title: {yaml_scalar(f'Topic: {tag}')}",
        GENERATED_MARKER,
        f"memento_topic: {yaml_scalar(tag)}",
        'tags: ["memento", "hub", "topic"]',
        "---",
        "",
        f"# Topic: {tag}",
        "",
        "<!-- memento:hub:start -->",
        f"> [[{root_hub.with_suffix('').as_posix()}|← Memento]] · {len(documents)} connected documents",
        "",
        "## Documents",
        "",
    ]
    lines.extend(f"- [[{document.target}|{document.title}]]" for document in documents)
    lines.extend(["", "<!-- memento:hub:end -->", ""])
    return normalize_markdown("\n".join(lines))


def _write_generated(root: Path, relative: Path, content: str, result: WikiLinkResult) -> bool:
    path = resolve_vault_path(root, relative)
    if path.exists():
        existing = path.read_text(encoding="utf-8", errors="replace")
        if GENERATED_MARKER not in existing:
            result.failed += 1
            result.warnings.append(f"refused to overwrite user-owned hub: {relative.as_posix()}")
            return False
        if existing == content:
            result.unchanged += 1
            return True
    atomic_write_text(path, content)
    return True


def _inject_navigation(
    document: WikiDocument,
    directory_hub: Path | None,
    tag_targets: list[tuple[str, Path]],
    result: WikiLinkResult,
) -> bool:
    try:
        content = document.path.read_text(encoding="utf-8", errors="replace")
        content = re.sub(
            rf"\n?{re.escape(NAV_START)}.*?{re.escape(NAV_END)}\n?",
            "\n",
            content,
            flags=re.DOTALL,
        )
        links = []
        if directory_hub:
            links.append(f"[[{directory_hub.with_suffix('').as_posix()}|↑ Hub]]")
        links.extend(f"[[{target.with_suffix('').as_posix()}|#{tag}]]" for tag, target in tag_targets)
        if not links:
            return False
        block = f"{NAV_START}\n> **Memento:** {' · '.join(links)}\n{NAV_END}\n"
        updated = _insert_after_frontmatter(content, block)
        updated = normalize_markdown(updated)
        if updated == normalize_markdown(document.path.read_text(encoding="utf-8", errors="replace")):
            result.unchanged += 1
            return False
        atomic_write_text(document.path, updated)
        return True
    except (OSError, UnicodeError) as error:
        result.failed += 1
        result.warnings.append(f"{document.relative.as_posix()}: {error}")
        return False


def _insert_after_frontmatter(content: str, block: str) -> str:
    normalized = content.lstrip("\ufeff")
    if normalized.startswith("---\n"):
        closing = normalized.find("\n---\n", 4)
        if closing != -1:
            boundary = closing + 5
            return f"{normalized[:boundary]}\n{block}\n{normalized[boundary:].lstrip()}"
    return f"{block}\n{normalized.lstrip()}"


def _frontmatter_title(content: str) -> str | None:
    frontmatter = _frontmatter(content)
    match = re.search(r'(?m)^title:\s*["\']?(.*?)["\']?\s*$', frontmatter)
    return match.group(1).strip() if match else None


def _extract_tags(content: str) -> set[str]:
    tags: set[str] = set()
    frontmatter = _frontmatter(content)
    for key in ("tags", "memento_tags"):
        inline = re.search(rf"(?m)^{key}:\s*\[(.*?)\]\s*$", frontmatter)
        if inline:
            tags.update(_clean_tag(value) for value in inline.group(1).split(","))
        block = re.search(rf"(?ms)^{key}:\s*\n((?:\s+-\s+.*\n?)+)", frontmatter)
        if block:
            tags.update(_clean_tag(line.partition("-")[2]) for line in block.group(1).splitlines())
    tags.update(_clean_tag(value) for value in re.findall(r"(?<![\w/])#([\wÀ-ÿ][\wÀ-ÿ/-]*)", content))
    return {tag for tag in tags if tag}


def _frontmatter(content: str) -> str:
    normalized = content.lstrip("\ufeff")
    if not normalized.startswith("---\n"):
        return ""
    closing = normalized.find("\n---\n", 4)
    return normalized[4:closing] if closing != -1 else ""


def _clean_tag(value: str) -> str:
    return value.strip().strip('"\'').lstrip("#").lower()


def _tag_hub_relative(tag: str) -> Path:
    return Path("_memento") / "topics" / f"{safe_filename(tag, fallback='topic')}.md"


def _directory_tags(
    documents: list[WikiDocument], by_tag: dict[str, list[WikiDocument]], minimum: int
) -> list[str]:
    tags = {tag for document in documents for tag in document.tags}
    return sorted(
        (tag for tag in tags if tag.lower() not in GENERIC_TAGS and len(by_tag[tag]) >= minimum),
        key=str.casefold,
    )

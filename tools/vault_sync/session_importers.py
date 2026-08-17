from __future__ import annotations

import json
import zipfile
from collections import Counter
from contextlib import suppress
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

from tools.vault_sync.config import LinkingConfig, SessionImportConfig, VaultConfig


def load_manifest(path: Path) -> dict[str, Any]:
    if path.exists():
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            return {}
    return {}


def save_manifest(path: Path, manifest: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8")


def normalize_title(value: str, fallback: str) -> str:
    title = (value or "").replace("\n", " ").strip()
    return title[:120] if title else fallback


def project_nav_line(linking: LinkingConfig, hub_target: str, hub_label: str, project_name: str) -> str:
    project_link = linking.resolve_project_link(project_name)
    if project_link:
        return f"> [[{hub_target}|← {hub_label}]] | [[{project_link}|→ {project_name}]]"
    return f"> [[{hub_target}|← {hub_label}]]"


@dataclass
class ImportResult:
    connector: str
    imported: int
    skipped: int
    hub_path: Path


def iter_connector_files(connector: SessionImportConfig) -> list[Path]:
    files = sorted(connector.source.rglob(connector.file_glob))
    if not connector.exclude_path_fragments:
        return files
    filtered = []
    for path in files:
        path_parts = {part.lower() for part in path.parts}
        if any(fragment.lower() in path_parts for fragment in connector.exclude_path_fragments):
            continue
        filtered.append(path)
    return filtered


def import_codex(vault: VaultConfig, connector: SessionImportConfig, linking: LinkingConfig) -> ImportResult:
    manifest = load_manifest(connector.manifest)
    imported = 0
    skipped = 0
    monthly_counts: Counter[str] = Counter()
    destination_root = vault.root / connector.destination
    destination_root.mkdir(parents=True, exist_ok=True)

    for session_file in iter_connector_files(connector):
        session_id = session_file.stem
        stat = session_file.stat()
        if manifest.get(session_id, {}).get("mtime") == stat.st_mtime:
            skipped += 1
            continue

        session = parse_codex_session(session_file)
        if not session:
            skipped += 1
            continue

        try:
            dt = datetime.strptime(session["date"], "%Y-%m-%d")
        except Exception:
            dt = datetime.now()
        month_dir = destination_root / dt.strftime("%Y-%m")
        month_dir.mkdir(parents=True, exist_ok=True)
        output = month_dir / f"{session_id}.md"
        output.write_text(render_codex_session(session, linking, connector), encoding="utf-8")

        manifest[session_id] = {
            "mtime": stat.st_mtime,
            "date": session["date"],
            "project": session["project"],
            "title": session["title"][:80],
            "path": str(output.relative_to(vault.root)),
        }
        monthly_counts[dt.strftime("%Y-%m")] += 1
        imported += 1

    save_manifest(connector.manifest, manifest)
    hub_path = write_monthly_hub(vault, connector, monthly_counts, title="# Codex Sessions")
    return ImportResult("codex", imported, skipped, hub_path)


def import_droid(vault: VaultConfig, connector: SessionImportConfig, linking: LinkingConfig) -> ImportResult:
    manifest = load_manifest(connector.manifest)
    imported = 0
    skipped = 0
    monthly_counts: Counter[str] = Counter()
    destination_root = vault.root / connector.destination
    destination_root.mkdir(parents=True, exist_ok=True)

    for session_file in iter_connector_files(connector):
        session_id = session_file.stem
        stat = session_file.stat()
        if manifest.get(session_id, {}).get("mtime") == stat.st_mtime:
            skipped += 1
            continue

        session = parse_droid_session(session_file)
        if not session:
            skipped += 1
            continue

        try:
            dt = datetime.strptime(session["date"], "%Y-%m-%d")
        except Exception:
            dt = datetime.now()
        month_dir = destination_root / dt.strftime("%Y-%m")
        month_dir.mkdir(parents=True, exist_ok=True)
        output = month_dir / f"{session_id}.md"
        output.write_text(render_droid_session(session, linking, connector), encoding="utf-8")

        manifest[session_id] = {
            "mtime": stat.st_mtime,
            "date": session["date"],
            "project": session["project"],
            "title": session["title"][:80],
            "path": str(output.relative_to(vault.root)),
        }
        monthly_counts[dt.strftime("%Y-%m")] += 1
        imported += 1

    save_manifest(connector.manifest, manifest)
    hub_path = write_monthly_hub(vault, connector, monthly_counts, title="# Droid Sessions")
    return ImportResult("droid", imported, skipped, hub_path)


def import_claude(vault: VaultConfig, connector: SessionImportConfig, linking: LinkingConfig) -> ImportResult:
    manifest = load_manifest(connector.manifest)
    imported = 0
    skipped = 0
    destination_root = vault.root / connector.destination
    destination_root.mkdir(parents=True, exist_ok=True)
    project_counts: Counter[str] = Counter()

    for session_file in iter_connector_files(connector):
        session_id = session_file.stem
        stat = session_file.stat()
        if manifest.get(session_id, {}).get("mtime") == stat.st_mtime:
            skipped += 1
            continue

        session = parse_claude_session(session_file)
        if not session:
            skipped += 1
            continue

        project_dir = destination_root / session["project_slug"]
        project_dir.mkdir(parents=True, exist_ok=True)
        output = project_dir / f"{session_id}.md"
        output.write_text(render_claude_session(session, linking, connector), encoding="utf-8")

        manifest[session_id] = {
            "mtime": stat.st_mtime,
            "date": session["date"],
            "project": session["project_slug"],
            "title": session["title"][:80],
            "path": str(output.relative_to(vault.root)),
        }
        project_counts[session["project_slug"]] += 1
        imported += 1

    save_manifest(connector.manifest, manifest)
    hub_path = write_project_hub(vault, connector, project_counts, title="# Claude Sessions")
    return ImportResult("claude", imported, skipped, hub_path)


def import_chatgpt(vault: VaultConfig, connector: SessionImportConfig, linking: LinkingConfig) -> ImportResult:
    manifest = load_manifest(connector.manifest)
    imported = 0
    skipped = 0
    monthly_counts: Counter[str] = Counter()
    destination_root = vault.root / connector.destination
    destination_root.mkdir(parents=True, exist_ok=True)

    for _batch_name, conversations in iter_chatgpt_batches(connector):
        for conversation in conversations:
            conversation_id = conversation.get("id", "")
            if not conversation_id:
                continue
            update_time = conversation.get("update_time", 0)
            if manifest.get(conversation_id, {}).get("update_time") == update_time:
                skipped += 1
                continue

            markdown = render_chatgpt_conversation(conversation, connector)
            if not markdown:
                skipped += 1
                continue

            create_time = conversation.get("create_time", 0)
            dt = datetime.fromtimestamp(create_time) if create_time else datetime.now()
            month_dir = destination_root / dt.strftime("%Y-%m")
            month_dir.mkdir(parents=True, exist_ok=True)
            output = month_dir / f"{conversation_id}.md"
            output.write_text(markdown, encoding="utf-8")

            manifest[conversation_id] = {
                "update_time": update_time,
                "date": dt.strftime("%Y-%m-%d"),
                "title": conversation.get("title", "")[:80],
                "path": str(output.relative_to(vault.root)),
            }
            monthly_counts[dt.strftime("%Y-%m")] += 1
            imported += 1

    save_manifest(connector.manifest, manifest)
    hub_path = write_monthly_hub(vault, connector, monthly_counts, title="# ChatGPT Sessions")
    return ImportResult("chatgpt", imported, skipped, hub_path)


def iter_chatgpt_batches(connector: SessionImportConfig) -> list[tuple[str, list[dict[str, Any]]]]:
    source = connector.source
    batches: list[tuple[str, list[dict[str, Any]]]] = []

    if source.is_file():
        if source.suffix.lower() == ".zip":
            try:
                with zipfile.ZipFile(source) as archive:
                    for name in sorted(archive.namelist()):
                        lowered = Path(name).name.lower()
                        if not (lowered == "conversations.json" or lowered.startswith("conversations-")):
                            continue
                        with archive.open(name) as handle:
                            payload = json.loads(handle.read().decode("utf-8"))
                        if isinstance(payload, list):
                            batches.append((name, payload))
            except Exception:
                return []
            return batches

        try:
            payload = json.loads(source.read_text(encoding="utf-8"))
        except Exception:
            return []
        if isinstance(payload, list):
            return [(source.name, payload)]
        return []

    if not source.exists():
        return []

    matched = sorted(source.glob(connector.file_glob))
    if not matched:
        fallback = source / "conversations.json"
        if fallback.exists():
            matched = [fallback]

    for batch_file in matched:
        try:
            payload = json.loads(batch_file.read_text(encoding="utf-8"))
        except Exception:
            continue
        if isinstance(payload, list):
            batches.append((batch_file.name, payload))
    return batches


def write_monthly_hub(
    vault: VaultConfig,
    connector: SessionImportConfig,
    counts: Counter[str],
    *,
    title: str,
) -> Path:
    destination_root = vault.root / connector.destination
    hub = [
        "---",
        f"source: {connector.source_tag}",
        "---",
        "",
        title,
        "",
    ]
    for month, count in sorted(counts.items(), reverse=True):
        hub.append(f"- [[{connector.destination.as_posix()}/{month}|{month}]] — {count} sessions")
    hub_path = destination_root / f"{connector.label} Hub.md"
    hub_path.write_text("\n".join(hub) + "\n", encoding="utf-8")
    return hub_path


def write_project_hub(
    vault: VaultConfig,
    connector: SessionImportConfig,
    counts: Counter[str],
    *,
    title: str,
) -> Path:
    destination_root = vault.root / connector.destination
    hub = [
        "---",
        f"source: {connector.source_tag}",
        "---",
        "",
        title,
        "",
    ]
    for project, count in sorted(counts.items(), key=lambda item: (-item[1], item[0])):
        hub.append(f"- [[{connector.destination.as_posix()}/{project}|{project}]] — {count} sessions")
    hub_path = destination_root / f"{connector.label} Hub.md"
    hub_path.write_text("\n".join(hub) + "\n", encoding="utf-8")
    return hub_path


def parse_codex_session(path: Path) -> dict[str, Any] | None:
    lines = []
    try:
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.strip():
                with suppress(Exception):
                    lines.append(json.loads(line))
    except Exception:
        return None
    if not lines:
        return None

    meta: dict[str, Any] = {}
    messages: list[dict[str, str]] = []
    create_time = None

    for record in lines:
        record_type = record.get("type", "")
        timestamp = record.get("timestamp", "")
        if not create_time and timestamp:
            create_time = timestamp

        if record_type == "session_meta":
            payload = record.get("payload", {})
            meta = {
                "session_id": payload.get("id", path.stem),
                "cwd": payload.get("cwd", ""),
            }
        elif record_type == "response_item":
            payload = record.get("payload", {})
            payload_type = payload.get("type", "")
            role = payload.get("role", "")
            content = payload.get("content", [])
            text = ""
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict):
                        if block.get("type") in {"input_text", "output_text"}:
                            text += block.get("text", "")
                    elif isinstance(block, str):
                        text += block
            elif isinstance(content, str):
                text = content

            if payload_type == "message" and text.strip():
                messages.append({"role": role or "unknown", "text": text.strip(), "ts": timestamp})

    if not messages and not meta:
        return None

    cwd = meta.get("cwd", "")
    project = Path(cwd).name if cwd else "unknown"
    title = next((message["text"] for message in messages if message["role"] == "user"), "")
    return {
        "session_id": meta.get("session_id", path.stem),
        "title": normalize_title(title, f"Codex {path.stem[:8]}"),
        "cwd": cwd,
        "project": project or "unknown",
        "date": create_time[:10] if create_time else "unknown",
        "messages": messages,
    }


def render_codex_session(session: dict[str, Any], linking: LinkingConfig, connector: SessionImportConfig) -> str:
    lines = [
        project_nav_line(
            linking, f"{connector.destination.as_posix()}/{connector.label} Hub.md", connector.label, session["project"]
        ),
        "",
        "---",
        f'title: "{session["title"].replace(chr(34), chr(39))}"',
        f"source: {connector.source_tag}",
        f"project: {session['project']}",
        f"session_id: {session['session_id']}",
        f"date: {session['date']}",
        f"cwd: {session['cwd']}",
        f"messages: {len(session['messages'])}",
        f"tags: [{connector.source_tag}, session, imported]",
        "---",
        "",
        f"# {session['title']}",
        "",
    ]
    for message in session["messages"]:
        prefix = "**User:**" if message["role"] == "user" else "**Assistant:**"
        lines.append(f"{prefix} {message['text']}")
        lines.append("")
    return "\n".join(lines)


def parse_droid_session(path: Path) -> dict[str, Any] | None:
    lines = []
    try:
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.strip():
                with suppress(Exception):
                    lines.append(json.loads(line))
    except Exception:
        return None
    if not lines:
        return None

    messages = []
    create_time = None
    cwd = ""
    session_id = path.stem
    title = ""

    for record in lines:
        record_type = record.get("type", "")
        timestamp = record.get("timestamp", "")
        if not create_time and timestamp:
            create_time = timestamp
        if record_type == "session_start":
            session_id = record.get("id", path.stem)
            title = record.get("title", "") or record.get("sessionTitle", "")
        elif record_type == "message":
            message = record.get("message", {})
            role = message.get("role", "unknown")
            content = message.get("content", [])
            text = ""
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "text":
                        raw = block.get("text", "")
                        if "Current folder:" in raw and not cwd:
                            cwd = raw.split("Current folder:", 1)[1].splitlines()[0].strip()
                        if "<system-reminder>" in raw:
                            continue
                        text += raw
                    elif isinstance(block, str):
                        text += block
            elif isinstance(content, str):
                text = content
            if text.strip() and role in {"user", "assistant"}:
                messages.append({"role": role, "text": text.strip()[:2000], "ts": timestamp})

    if not messages:
        return None
    project = Path(cwd).name if cwd else "unknown"
    first_user = next((message["text"] for message in messages if message["role"] == "user"), "")
    return {
        "session_id": session_id,
        "title": normalize_title(title or first_user, f"Droid {session_id[:8]}"),
        "cwd": cwd,
        "project": project,
        "date": create_time[:10] if create_time else "unknown",
        "messages": messages,
    }


def render_droid_session(session: dict[str, Any], linking: LinkingConfig, connector: SessionImportConfig) -> str:
    lines = [
        project_nav_line(
            linking, f"{connector.destination.as_posix()}/{connector.label} Hub.md", connector.label, session["project"]
        ),
        "",
        "---",
        f'title: "{session["title"].replace(chr(34), chr(39))}"',
        f"source: {connector.source_tag}",
        f"project: {session['project']}",
        f"session_id: {session['session_id']}",
        f"date: {session['date']}",
        f"cwd: {session['cwd']}",
        f"messages: {len(session['messages'])}",
        f"tags: [{connector.source_tag}, session, imported]",
        "---",
        "",
        f"# {session['title']}",
        "",
    ]
    for message in session["messages"]:
        prefix = "**User:**" if message["role"] == "user" else "**Assistant:**"
        lines.append(f"{prefix} {message['text']}")
        lines.append("")
    return "\n".join(lines)


def parse_claude_session(path: Path) -> dict[str, Any] | None:
    lines = []
    try:
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.strip():
                with suppress(Exception):
                    lines.append(json.loads(line))
    except Exception:
        return None
    if not lines:
        return None

    nodes = {}
    first = lines[0]
    session_id = first.get("sessionId", path.stem)
    cwd = first.get("cwd", "")
    create_time = first.get("timestamp", "")

    for record in lines:
        uuid = record.get("uuid")
        if uuid:
            nodes[uuid] = record

    roots = [node for node in nodes.values() if node.get("parentUuid") is None]
    thread = []
    visited = set()
    children_by_parent: dict[str, list[dict[str, Any]]] = {}
    for node in nodes.values():
        parent_uuid = node.get("parentUuid")
        if parent_uuid:
            children_by_parent.setdefault(parent_uuid, []).append(node)

    for root in roots:
        current: dict[str, Any] | None = root
        while current is not None:
            uuid = current.get("uuid", "")
            if uuid in visited:
                break
            visited.add(uuid)
            thread.append(current)
            children = children_by_parent.get(uuid, [])
            current = children[0] if children else None

    messages = []
    for record in thread:
        message = record.get("message", {})
        if not message:
            continue
        role = message.get("role", record.get("type", "unknown"))
        content = message.get("content", "")
        text_parts = []
        if isinstance(content, list):
            for block in content:
                if isinstance(block, str):
                    text_parts.append(block)
                elif isinstance(block, dict):
                    if block.get("type") == "text":
                        text_parts.append(block.get("text", ""))
                    elif block.get("type") == "tool_use":
                        text_parts.append(f"[tool: {block.get('name', '?')}]")
        elif isinstance(content, str):
            text_parts.append(content)
        text = "\n".join(part for part in text_parts if part).strip()
        if text:
            messages.append({"role": role, "text": text, "ts": record.get("timestamp", "")})

    if not messages:
        return None

    project_slug = path.parent.parent.name.lstrip("-")
    title = next((message["text"] for message in messages if message["role"] == "user"), "")
    return {
        "session_id": session_id,
        "title": normalize_title(title, f"Claude {session_id[:8]}"),
        "cwd": cwd,
        "project": Path(cwd).name if cwd else "unknown",
        "project_slug": project_slug,
        "date": create_time[:10] if create_time else "unknown",
        "messages": messages,
    }


def render_claude_session(session: dict[str, Any], linking: LinkingConfig, connector: SessionImportConfig) -> str:
    lines = [
        project_nav_line(
            linking, f"{connector.destination.as_posix()}/{connector.label} Hub.md", connector.label, session["project"]
        ),
        "",
        "---",
        f'title: "{session["title"].replace(chr(34), chr(39))}"',
        f"source: {connector.source_tag}",
        f"project: {session['project']}",
        f"session_id: {session['session_id']}",
        f"date: {session['date']}",
        f"cwd: {session['cwd']}",
        f"messages: {len(session['messages'])}",
        f"tags: [{connector.source_tag}, session, imported]",
        "---",
        "",
        f"# {session['title']}",
        "",
    ]
    for message in session["messages"]:
        if message["role"] == "user":
            prefix = "**User:**"
        elif message["role"] == "assistant":
            prefix = "**Claude:**"
        else:
            prefix = f"**{message['role'].title()}:**"
        lines.append(f"{prefix} {message['text']}")
        lines.append("")
    return "\n".join(lines)


def extract_chatgpt_messages(mapping: dict[str, Any], current_node: str) -> list[dict[str, Any]]:
    chain = []
    node_id = current_node
    visited = set()
    while node_id and node_id not in visited:
        visited.add(node_id)
        node = mapping.get(node_id, {})
        message = node.get("message")
        if message and message.get("content"):
            text = ""
            for part in message["content"].get("parts", []):
                if isinstance(part, str):
                    text += part
                elif isinstance(part, dict) and part.get("content_type") == "text":
                    text += part.get("text", "")
            if text.strip():
                chain.append(
                    {
                        "role": message.get("author", {}).get("role", "unknown"),
                        "text": text.strip(),
                        "ts": message.get("create_time"),
                    }
                )
        node_id = node.get("parent")
    chain.reverse()
    return chain


def render_chatgpt_conversation(conversation: dict[str, Any], connector: SessionImportConfig) -> str:
    messages = extract_chatgpt_messages(conversation.get("mapping", {}), conversation.get("current_node", ""))
    if not messages:
        return ""
    title = normalize_title(conversation.get("title", ""), "ChatGPT Conversation")
    create_time = conversation.get("create_time", 0)
    dt = datetime.fromtimestamp(create_time) if create_time else datetime.now()
    lines = [
        f"> [[{connector.destination.as_posix()}/{connector.label} Hub.md|← {connector.label}]]",
        "",
        "---",
        f'title: "{title.replace(chr(34), chr(39))}"',
        f"source: {connector.source_tag}",
        f"date: {dt.strftime('%Y-%m-%d')}",
        f"conversation_id: {conversation.get('id', 'unknown')}",
        f"messages: {len(messages)}",
        f"tags: [{connector.source_tag}, session, imported]",
        "---",
        "",
        f"# {title}",
        "",
    ]
    for message in messages:
        prefix = "**User:**" if message["role"] == "user" else "**ChatGPT:**"
        lines.append(f"{prefix} {message['text']}")
        lines.append("")
    return "\n".join(lines)

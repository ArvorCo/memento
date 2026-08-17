from __future__ import annotations

import json
import re
import shutil
import tempfile
import zipfile
from collections import defaultdict
from dataclasses import dataclass
from datetime import date, datetime
from pathlib import Path

from tools.vault_sync.config import VaultConfig, WhatsAppImportConfig

MSG_RE = re.compile(r"^\[(\d{1,2}/\d{1,2}/\d{2,4}),\s*(\d{2}:\d{2}:\d{2})\]\s*([^:]+):\s*(.*)")


@dataclass
class WhatsAppImportResult:
    imported: int = 0
    skipped: int = 0
    messages: int = 0
    media_files: int = 0
    failed: int = 0


def slugify(name: str) -> str:
    return re.sub(r"[^\w\s\-]", "", name, flags=re.UNICODE).strip() or "chat"


def load_manifest(path: Path) -> dict:
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}


def save_manifest(path: Path, manifest: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8")


def parse_date(date_str: str) -> date | None:
    parts = date_str.split("/")
    if len(parts) != 3:
        return None
    day, month, year = parts
    year_int = int(year)
    if year_int < 100:
        year_int += 2000
    try:
        return date(year_int, int(month), int(day))
    except ValueError:
        return None


def parse_chat(chat_text: str) -> list[dict]:
    messages = []
    current = None
    for raw_line in chat_text.splitlines():
        line = raw_line.rstrip("\r\n")
        match = MSG_RE.match(line)
        if match:
            if current:
                messages.append(current)
            current = {
                "date": parse_date(match.group(1)),
                "time": match.group(2),
                "sender": match.group(3).strip(),
                "text": match.group(4),
            }
        elif current is not None:
            current["text"] += "\n" + line
    if current:
        messages.append(current)
    return messages


def resolve_category(chat_name: str, config: WhatsAppImportConfig) -> Path:
    lowered = chat_name.lower()
    for rule in config.category_rules:
        if any(token.lower() in lowered for token in rule.matches):
            return rule.destination
    return config.default_category


def render_attachment_reference(filename: str) -> str:
    return f"[{filename}](./media/{filename})"


def render_message(message: dict) -> str:
    text = message["text"]
    attachment_match = re.search(r"(\d{5}-[\w\.\-]+\.\w+)", text)
    if attachment_match:
        text = text.replace(attachment_match.group(1), render_attachment_reference(attachment_match.group(1)))
    return f"**{message['time'][:5]}** `{message['sender']}:` {text}"


def write_period_markdown(path: Path, chat_name: str, category: Path, period_key: str, messages: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "---",
        f"chat: {chat_name}",
        f"period: {period_key}",
        f"tags: [whatsapp, imported, {category.as_posix().replace('/', '-')}]",
        "---",
        "",
        f"# {chat_name} — {period_key}",
        "",
    ]
    current_day = None
    for message in messages:
        if message["date"] != current_day:
            current_day = message["date"]
            label = current_day.isoformat() if current_day else "unknown"
            lines.extend(["", f"## {label}", ""])
        lines.append(render_message(message))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_chat_hub(chat_dir: Path, chat_name: str, category: Path, period_counts: dict[str, int]) -> Path:
    hub = chat_dir / "_hub.md"
    lines = [
        "---",
        f"chat: {chat_name}",
        f"tags: [whatsapp, hub, {category.as_posix().replace('/', '-')}]",
        "---",
        "",
        f"# {chat_name}",
        "",
    ]
    for period, count in sorted(period_counts.items()):
        lines.append(f"- [[{period}|{period}]] — {count} messages")
    hub.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return hub


def import_whatsapp(vault: VaultConfig, config: WhatsAppImportConfig) -> WhatsAppImportResult:
    result = WhatsAppImportResult()
    if not config.enabled:
        return result

    manifest = load_manifest(config.manifest)
    if config.source.is_file():
        zips = [config.source] if config.source.suffix.lower() == ".zip" else []
    else:
        zips = sorted(config.source.glob("*.zip"))
    for zip_path in zips:
        mtime = int(zip_path.stat().st_mtime)
        if manifest.get(zip_path.name, {}).get("mtime") == mtime:
            result.skipped += 1
            continue
        stats = process_whatsapp_zip(vault, config, zip_path)
        if stats is None:
            result.failed += 1
            continue
        manifest[zip_path.name] = {"mtime": mtime, **stats}
        result.imported += 1
        result.messages += stats["messages"]
        result.media_files += stats["media_files"]

    save_manifest(config.manifest, manifest)
    return result


def process_whatsapp_zip(vault: VaultConfig, config: WhatsAppImportConfig, zip_path: Path) -> dict | None:
    chat_name = re.sub(r"^WhatsApp Chat - ", "", zip_path.stem).strip()
    category = resolve_category(chat_name, config)
    with tempfile.TemporaryDirectory(prefix="memento_whatsapp_") as tempdir:
        temp_path = Path(tempdir)
        try:
            with zipfile.ZipFile(zip_path, "r") as archive:
                archive.extractall(temp_path)
        except Exception:
            return None

        chat_files = list(temp_path.rglob("_chat.txt"))
        if not chat_files:
            return None

        messages = parse_chat(chat_files[0].read_text(encoding="utf-8-sig", errors="replace"))
        if not messages:
            return None

        chat_dir = vault.root / config.destination / category / slugify(chat_name)
        media_dir = chat_dir / "media"
        media_dir.mkdir(parents=True, exist_ok=True)
        media_files = 0
        for path in temp_path.iterdir():
            if not path.is_file() or path.name == "_chat.txt":
                continue
            shutil.copy2(path, media_dir / path.name)
            media_files += 1

        by_month: dict[str, list[dict]] = defaultdict(list)
        for message in messages:
            if message["date"] is None:
                continue
            by_month[message["date"].strftime("%Y-%m")].append(message)

        period_counts = {}
        for period, period_messages in sorted(by_month.items()):
            period_counts[period] = len(period_messages)
            write_period_markdown(chat_dir / f"{period}.md", chat_name, category, period, period_messages)

        write_chat_hub(chat_dir, chat_name, category, period_counts)
        return {
            "messages": len(messages),
            "media_files": media_files,
            "category": category.as_posix(),
            "chat": chat_name,
            "processed_at": datetime.now().isoformat(timespec="seconds"),
        }

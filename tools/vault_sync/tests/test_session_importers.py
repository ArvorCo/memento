from __future__ import annotations

import json
import tempfile
import unittest
import zipfile
from pathlib import Path

from tools.vault_sync.config import LinkingConfig, SessionImportConfig, VaultConfig
from tools.vault_sync.session_importers import (
    extract_chatgpt_messages,
    import_codex,
    iter_chatgpt_batches,
    iter_connector_files,
    parse_claude_session,
    parse_codex_session,
    parse_droid_session,
    render_chatgpt_conversation,
)

FIXTURES = Path(__file__).resolve().parent / "fixtures"


class SessionImporterTests(unittest.TestCase):
    def test_parse_codex_session(self) -> None:
        session = parse_codex_session(FIXTURES / "codex_session.jsonl")
        assert session is not None
        self.assertEqual(session["project"], "memento")
        self.assertEqual(session["messages"][0]["role"], "user")

    def test_parse_droid_session(self) -> None:
        session = parse_droid_session(FIXTURES / "droid_session.jsonl")
        assert session is not None
        self.assertEqual(session["project"], "memento")
        self.assertEqual(len(session["messages"]), 2)

    def test_parse_claude_session_handles_deep_threads_iteratively(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            path = Path(tempdir) / "deep.jsonl"
            records = []
            parent = None
            for index in range(1_500):
                uuid = f"node-{index}"
                records.append(
                    {
                        "uuid": uuid,
                        "parentUuid": parent,
                        "sessionId": "deep-session",
                        "cwd": "/tmp/project",
                        "timestamp": "2026-08-17T00:00:00Z",
                        "message": {"role": "user", "content": f"message {index}"},
                    }
                )
                parent = uuid
            path.write_text("\n".join(json.dumps(record) for record in records), encoding="utf-8")

            session = parse_claude_session(path)

            assert session is not None
            self.assertEqual(len(session["messages"]), 1_500)

    def test_chatgpt_render(self) -> None:
        conversation = json.loads((FIXTURES / "chatgpt_conversation.json").read_text(encoding="utf-8"))
        messages = extract_chatgpt_messages(conversation["mapping"], conversation["current_node"])
        self.assertEqual(len(messages), 2)
        connector = SessionImportConfig(
            enabled=True,
            source=Path("/tmp/src"),
            destination=Path("converted/chatgpt"),
            manifest=Path("/tmp/manifest.json"),
            label="ChatGPT",
            source_tag="chatgpt",
            file_glob="conversations*.json",
            exclude_path_fragments=[],
        )
        markdown = render_chatgpt_conversation(conversation, connector)
        self.assertIn("# Simple Chat", markdown)
        self.assertIn("**User:**", markdown)

    def test_codex_import_is_incremental_on_second_run(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            source = root / "sessions"
            source.mkdir(parents=True)
            session_file = source / "sample.jsonl"
            session_file.write_text(
                (FIXTURES / "codex_session.jsonl").read_text(encoding="utf-8"),
                encoding="utf-8",
            )

            vault = VaultConfig(root / "vault", root / "state")
            connector = SessionImportConfig(
                enabled=True,
                source=source,
                destination=Path("converted/codex"),
                manifest=root / "state" / "codex_manifest.json",
                label="Codex",
                source_tag="codex",
                file_glob="*.jsonl",
                exclude_path_fragments=[],
            )
            linking = LinkingConfig(enabled=False, default_project_prefix="projects", project_aliases={})

            first = import_codex(vault, connector, linking)
            second = import_codex(vault, connector, linking)

            self.assertEqual(first.imported, 1)
            self.assertEqual(second.imported, 0)
            self.assertEqual(second.skipped, 1)

    def test_iter_chatgpt_batches_supports_conversations_json_file(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            export_file = root / "conversations.json"
            payload = [json.loads((FIXTURES / "chatgpt_conversation.json").read_text(encoding="utf-8"))]
            export_file.write_text(json.dumps(payload), encoding="utf-8")
            connector = SessionImportConfig(
                enabled=True,
                source=export_file,
                destination=Path("converted/chatgpt"),
                manifest=root / "manifest.json",
                label="ChatGPT",
                source_tag="chatgpt",
                file_glob="conversations*.json",
                exclude_path_fragments=[],
            )
            batches = iter_chatgpt_batches(connector)
            self.assertEqual(len(batches), 1)
            self.assertEqual(len(batches[0][1]), 1)

    def test_iter_chatgpt_batches_supports_zip_export(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            export_zip = root / "chatgpt-export.zip"
            payload = [json.loads((FIXTURES / "chatgpt_conversation.json").read_text(encoding="utf-8"))]
            with zipfile.ZipFile(export_zip, "w") as archive:
                archive.writestr("export/conversations.json", json.dumps(payload))
            connector = SessionImportConfig(
                enabled=True,
                source=export_zip,
                destination=Path("converted/chatgpt"),
                manifest=root / "manifest.json",
                label="ChatGPT",
                source_tag="chatgpt",
                file_glob="conversations*.json",
                exclude_path_fragments=[],
            )
            batches = iter_chatgpt_batches(connector)
            self.assertEqual(len(batches), 1)
            self.assertEqual(len(batches[0][1]), 1)

    def test_iter_connector_files_respects_excluded_path_fragments(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            (root / "main").mkdir()
            (root / "subagents").mkdir()
            (root / "main" / "session.jsonl").write_text("{}", encoding="utf-8")
            (root / "subagents" / "agent.jsonl").write_text("{}", encoding="utf-8")
            connector = SessionImportConfig(
                enabled=True,
                source=root,
                destination=Path("converted/claude"),
                manifest=root / "manifest.json",
                label="Claude",
                source_tag="claude",
                file_glob="*.jsonl",
                exclude_path_fragments=["subagents"],
            )
            files = iter_connector_files(connector)
            self.assertEqual([path.name for path in files], ["session.jsonl"])


if __name__ == "__main__":
    unittest.main()

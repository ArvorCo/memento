from __future__ import annotations

import tempfile
import textwrap
import unittest
from pathlib import Path

from tools.vault_sync.config import load_config


class ConfigTests(unittest.TestCase):
    def test_disabled_connectors_accept_minimal_sections(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            config_path = root / "config.toml"
            config_path.write_text(
                textwrap.dedent(
                    f"""
                    [vault]
                    root = "{root / 'vault'}"
                    state_dir = "{root / 'state'}"

                    [icloud_sync]
                    enabled = false

                    [apple_notes]
                    enabled = false

                    [whatsapp_import]
                    enabled = false
                    """
                ),
                encoding="utf-8",
            )

            config = load_config(config_path)

            self.assertIsNone(config.icloud)
            self.assertIsNone(config.apple_notes)
            self.assertIsNone(config.whatsapp)

    def test_load_config_rejects_destination_outside_vault(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            config_path = Path(tempdir) / "config.toml"
            config_path.write_text(
                textwrap.dedent(
                    """
                    [vault]
                    root = "~/vault"
                    state_dir = "~/state-dir"

                    [[document_import.sources]]
                    name = "unsafe"
                    source = "~/Documents"
                    destination = "../../outside"
                    """
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "inside the vault"):
                load_config(config_path)

    def test_load_config_resolves_relative_manifest_under_state_dir(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            config_path = Path(tempdir) / "config.toml"
            config_path.write_text(
                textwrap.dedent(
                    """
                    [vault]
                    root = "~/vault"
                    state_dir = "~/state-dir"

                    [linking]
                    enabled = true
                    default_project_prefix = "projects"

                    [session_import.codex]
                    enabled = true
                    source = "~/.codex/sessions"
                    destination = "converted/codex"
                    manifest = "codex_manifest.json"
                    label = "Codex"
                    source_tag = "codex"
                    file_glob = "*.jsonl"
                    exclude_path_fragments = ["subagents"]
                    """
                ),
                encoding="utf-8",
            )
            config = load_config(config_path)
            self.assertTrue(str(config.session_imports["codex"].manifest).endswith("state-dir/codex_manifest.json"))
            self.assertEqual(config.linking.default_project_prefix, "projects")
            self.assertEqual(config.session_imports["codex"].file_glob, "*.jsonl")
            self.assertEqual(config.session_imports["codex"].exclude_path_fragments, ["subagents"])

    def test_load_config_parses_connector_sections(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            config_path = Path(tempdir) / "config.toml"
            config_path.write_text(
                textwrap.dedent(
                    """
                    [vault]
                    root = "~/vault"
                    state_dir = "~/state-dir"

                    [linking]
                    enabled = false
                    default_project_prefix = "projects"

                    [icloud_sync]
                    enabled = true
                    root = "~/Library/Mobile Documents/com~apple~CloudDocs"

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
                    enabled = true
                    destination = "converted/apple-notes"
                    include_index = true

                    [whatsapp_import]
                    enabled = true
                    source = "~/Downloads"
                    destination = "whatsapp"
                    manifest = "whatsapp_manifest.json"
                    default_category = "outros"

                    [[whatsapp_import.category_rules]]
                    name = "work"
                    destination = "work"
                    matches = ["team"]
                    """
                ),
                encoding="utf-8",
            )
            config = load_config(config_path)
            self.assertIsNotNone(config.icloud)
            self.assertTrue(config.icloud.enabled)
            self.assertEqual(config.icloud.folders[0].source.as_posix(), "Documents")
            self.assertIsNotNone(config.apple_notes)
            self.assertEqual(config.apple_notes.destination.as_posix(), "converted/apple-notes")
            self.assertIsNotNone(config.whatsapp)
            self.assertEqual(config.whatsapp.default_category.as_posix(), "outros")
            self.assertEqual(config.whatsapp.category_rules[0].destination.as_posix(), "work")


if __name__ == "__main__":
    unittest.main()

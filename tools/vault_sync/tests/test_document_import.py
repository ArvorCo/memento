from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

from tools.vault_sync.config import DocumentImportConfig, VaultConfig
from tools.vault_sync.document_import import import_documents


class DocumentImportTests(unittest.TestCase):
    def test_import_is_hash_incremental_and_removes_tracked_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            source = root / "source"
            source.mkdir()
            note = source / "note.txt"
            note.write_text("alpha", encoding="utf-8")
            original_stat = note.stat()
            config = DocumentImportConfig(
                name="docs",
                enabled=True,
                source=source,
                destination=Path("converted/docs"),
                manifest=root / "state" / "docs.json",
                include_extensions={".txt"},
                exclude_dirs=set(),
                preserve_raw=False,
                raw_destination=None,
                delete_removed=True,
                tags=["documents"],
                max_file_bytes=1024,
            )
            vault = VaultConfig(root / "vault", root / "state")

            first = import_documents(vault, config)
            second = import_documents(vault, config)
            note.write_text("bravo", encoding="utf-8")
            os.utime(note, ns=(original_stat.st_atime_ns, original_stat.st_mtime_ns))
            third = import_documents(vault, config)

            output = vault.root / "converted/docs/note.txt.md"
            self.assertEqual(first.imported, 1)
            self.assertEqual(second.skipped, 1)
            self.assertEqual(third.updated, 1)
            self.assertIn("bravo", output.read_text(encoding="utf-8"))
            self.assertIn("memento_source_sha256", output.read_text(encoding="utf-8"))

            note.unlink()
            fourth = import_documents(vault, config)
            self.assertEqual(fourth.removed, 1)
            self.assertFalse(output.exists())

    def test_markdown_frontmatter_is_preserved_and_augmented(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            source = root / "source"
            source.mkdir()
            (source / "note.md").write_text("---\ntags: [alpha]\n---\n# Note\nBody\n", encoding="utf-8")
            config = DocumentImportConfig(
                name="docs",
                enabled=True,
                source=source,
                destination=Path("docs"),
                manifest=root / "manifest.json",
                include_extensions={".md"},
                exclude_dirs=set(),
                preserve_raw=False,
                raw_destination=None,
                delete_removed=True,
                tags=["imported"],
                max_file_bytes=1024,
            )

            result = import_documents(VaultConfig(root / "vault", root / "state"), config)
            output = (root / "vault/docs/note.md").read_text(encoding="utf-8")

            self.assertEqual(result.imported, 1)
            self.assertEqual(output.count("tags: [alpha]"), 1)
            self.assertIn("memento_tags", output)
            self.assertIn("# Note", output)


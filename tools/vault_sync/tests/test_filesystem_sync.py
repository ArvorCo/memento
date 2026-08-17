from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.vault_sync.config import MarkdownSyncRoot, VaultConfig
from tools.vault_sync.filesystem_sync import sync_markdown_root


class FilesystemSyncTests(unittest.TestCase):
    def test_sync_markdown_copies_and_deletes_unprotected_files(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            source = temp / "source"
            vault = temp / "vault"
            (source / "docs").mkdir(parents=True)
            (source / "docs" / "old.md").write_text("# old\n", encoding="utf-8")

            root = MarkdownSyncRoot(
                name="workspace",
                source=source,
                destination=Path("projects"),
                include_extensions=[".md"],
                exclude_dirs={".git"},
                protected_globs=["**/_*_hub.md"],
            )
            first = sync_markdown_root(VaultConfig(vault, temp / "state"), root)
            (source / "docs" / "old.md").unlink()
            (source / "docs" / "a.md").write_text("# A\n", encoding="utf-8")
            result = sync_markdown_root(VaultConfig(vault, temp / "state"), root)
            self.assertEqual(first.copied, 1)
            self.assertEqual(result.copied, 1)
            self.assertEqual(result.deleted, 1)
            self.assertTrue((vault / "projects" / "docs" / "a.md").exists())

    def test_sync_markdown_preserves_protected_files(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            source = temp / "source"
            vault = temp / "vault"
            source.mkdir(parents=True)
            (vault / "projects").mkdir(parents=True)
            (vault / "projects" / "_main_hub.md").write_text("# hub\n", encoding="utf-8")

            root = MarkdownSyncRoot(
                name="workspace",
                source=source,
                destination=Path("projects"),
                include_extensions=[".md"],
                exclude_dirs=set(),
                protected_globs=["**/_*_hub.md"],
            )
            sync_markdown_root(VaultConfig(vault, temp / "state"), root)
            self.assertTrue((vault / "projects" / "_main_hub.md").exists())

    def test_sync_markdown_skips_files_that_disappear_mid_copy(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            source = temp / "source"
            vault = temp / "vault"
            source.mkdir(parents=True)
            (source / "note.md").write_text("# Note\n", encoding="utf-8")

            root = MarkdownSyncRoot(
                name="workspace",
                source=source,
                destination=Path("projects"),
                include_extensions=[".md"],
                exclude_dirs=set(),
                protected_globs=[],
            )

            with mock.patch(
                "tools.vault_sync.filesystem_sync.atomic_copy",
                side_effect=FileNotFoundError,
            ):
                result = sync_markdown_root(VaultConfig(vault, temp / "state"), root)

            self.assertEqual(result.copied, 0)
            self.assertEqual(result.skipped, 1)


if __name__ == "__main__":
    unittest.main()

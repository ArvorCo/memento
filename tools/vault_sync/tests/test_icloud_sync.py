from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

from tools.vault_sync.config import ICloudFolderConfig, ICloudSyncConfig, VaultConfig
from tools.vault_sync.icloud_sync import sync_icloud


class ICloudSyncTests(unittest.TestCase):
    def test_sync_icloud_copies_markdown_and_text(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            icloud_root = root / "icloud"
            source = icloud_root / "Documents"
            source.mkdir(parents=True)
            (source / "note.md").write_text("# note\n", encoding="utf-8")
            (source / "memo.txt").write_text("hello\n", encoding="utf-8")

            config = ICloudSyncConfig(
                enabled=True,
                root=icloud_root,
                folders=[
                    ICloudFolderConfig(
                        name="documents",
                        source=Path("Documents"),
                        raw_destination=Path("raw/icloud/documents"),
                        converted_destination=Path("converted/icloud/documents"),
                        include_markdown=True,
                        include_text=True,
                        convert_doc=True,
                        convert_docx=True,
                        convert_pptx=False,
                        convert_pdf=True,
                    )
                ],
            )

            result = sync_icloud(VaultConfig(root / "vault", root / "state"), config)
            self.assertEqual(result.copied, 2)
            self.assertTrue((root / "vault/raw/icloud/documents/note.md").exists())
            self.assertTrue((root / "vault/raw/icloud/documents/memo.txt").exists())

            note = source / "note.md"
            original_mtime = note.stat().st_mtime_ns
            note.write_text("# mote\n", encoding="utf-8")
            os.utime(note, ns=(original_mtime, original_mtime))
            second = sync_icloud(VaultConfig(root / "vault", root / "state"), config)

            self.assertEqual(second.copied, 1)
            self.assertEqual(second.skipped, 1)
            self.assertEqual(
                (root / "vault/raw/icloud/documents/note.md").read_text(encoding="utf-8"),
                "# mote\n",
            )


if __name__ == "__main__":
    unittest.main()

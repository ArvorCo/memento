from __future__ import annotations

import tempfile
import unittest
import zipfile
from pathlib import Path

from tools.vault_sync.config import VaultConfig, WhatsAppCategoryRule, WhatsAppImportConfig
from tools.vault_sync.whatsapp_import import import_whatsapp


class WhatsAppImportTests(unittest.TestCase):
    def test_import_whatsapp_zip_writes_chat_month_and_hub(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            downloads = root / "downloads"
            downloads.mkdir()
            zip_path = downloads / "WhatsApp Chat - Work Team.zip"
            with zipfile.ZipFile(zip_path, "w") as archive:
                archive.writestr(
                    "_chat.txt",
                    "\n".join(
                        [
                            "[01/04/2026, 10:00:00] Alice: hello",
                            "[01/04/2026, 10:05:00] Bob: 00001-PHOTO-2026.jpg (file attached)",
                        ]
                    ),
                )
                archive.writestr("00001-PHOTO-2026.jpg", b"fake-image")

            config = WhatsAppImportConfig(
                enabled=True,
                source=downloads,
                destination=Path("whatsapp"),
                manifest=root / "state" / "whatsapp_manifest.json",
                category_rules=[WhatsAppCategoryRule(name="work", destination=Path("work"), matches=["work", "team"])],
                default_category=Path("outros"),
            )
            result = import_whatsapp(VaultConfig(root / "vault", root / "state"), config)
            self.assertEqual(result.imported, 1)
            self.assertEqual(result.messages, 2)
            chat_dir = root / "vault" / "whatsapp" / "work" / "Work Team"
            self.assertTrue((chat_dir / "2026-04.md").exists())
            self.assertTrue((chat_dir / "_hub.md").exists())
            self.assertTrue((chat_dir / "media" / "00001-PHOTO-2026.jpg").exists())

    def test_import_whatsapp_accepts_single_zip_file_source(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            zip_path = root / "WhatsApp Chat - Family.zip"
            with zipfile.ZipFile(zip_path, "w") as archive:
                archive.writestr("_chat.txt", "[01/04/2026, 10:00:00] Alice: oi\n")

            config = WhatsAppImportConfig(
                enabled=True,
                source=zip_path,
                destination=Path("whatsapp"),
                manifest=root / "state" / "whatsapp_manifest.json",
                category_rules=[],
                default_category=Path("outros"),
            )
            result = import_whatsapp(VaultConfig(root / "vault", root / "state"), config)
            self.assertEqual(result.imported, 1)
            self.assertTrue(any((root / "vault/whatsapp").rglob("_hub.md")))


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools.vault_sync.apple_notes import convert_exported_notes


class AppleNotesTests(unittest.TestCase):
    def test_convert_exported_notes_writes_markdown_and_index(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            raw = root / "raw"
            raw.mkdir()
            (raw / "1.txt").write_text(
                "Work\nMeeting Notes\n<div><b>Hello</b><br>World</div>",
                encoding="utf-8",
            )
            destination = root / "vault" / "converted" / "apple-notes"
            result = convert_exported_notes(raw, destination, include_index=True)
            self.assertEqual(result.exported, 1)
            self.assertEqual(result.converted, 1)
            note = destination / "Work" / "Meeting Notes.md"
            self.assertTrue(note.exists())
            content = note.read_text(encoding="utf-8")
            self.assertIn("**Hello**", content)
            self.assertTrue((destination / "_INDEX.md").exists())


if __name__ == "__main__":
    unittest.main()

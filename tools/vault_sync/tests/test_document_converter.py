from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools.vault_sync.document_converter import _clean_pdf_pages, convert_document


class DocumentConverterTests(unittest.TestCase):
    def test_csv_conversion_preserves_rows_as_markdown_table(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            path = Path(tempdir) / "people.csv"
            path.write_text('name,role\nAda,"engineer|researcher"\n', encoding="utf-8")

            result = convert_document(path)

            self.assertEqual(result.converter, "python-csv")
            self.assertIn("| name | role |", result.markdown)
            self.assertIn(r"engineer\|researcher", result.markdown)

    def test_pdf_cleanup_removes_repeated_edges_and_dehyphenates_words(self) -> None:
        pages = [
            "Quarterly Report\ninter-\nnational results\n1",
            "Quarterly Report\nsecond page\n2",
            "Quarterly Report\nthird page\n3",
        ]

        cleaned = _clean_pdf_pages(pages)

        self.assertNotIn("Quarterly Report", "\n".join(cleaned))
        self.assertIn("international results", cleaned[0])


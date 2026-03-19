from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from pdf_foundry.parsing.pdf_parser import normalize_markdown, parse_incremental_pdf_dir


class ParserTests(unittest.TestCase):
    def test_normalize_markdown_strips_repeated_headers(self) -> None:
        markdown = "\n".join(["Report Header"] * 10 + ["# Finding 1", "Issue details."])
        normalized = normalize_markdown(markdown)
        self.assertIn("# Finding 1", normalized)
        self.assertNotIn("Report Header\nReport Header\nReport Header", normalized)

    def test_parse_incremental_skips_unchanged_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            pdf_dir = root / "pdfs"
            md_dir = root / "markdown"
            manifest_path = root / "manifest.json"
            pdf_dir.mkdir(parents=True, exist_ok=True)
            pdf = pdf_dir / "a.pdf"
            pdf.write_bytes(b"%PDF test")

            with patch(
                "pdf_foundry.parsing.pdf_parser.parse_pdf_to_markdown",
                return_value="# Heading\ncontent\n",
            ):
                changed, manifest = parse_incremental_pdf_dir(
                    pdf_dir=pdf_dir, markdown_dir=md_dir, manifest_path=manifest_path
                )
                self.assertEqual(len(changed), 1)
                self.assertEqual(len(manifest), 1)

            manifest_path.write_text('{"a.pdf": "' + next(iter(manifest.values())) + '"}', encoding="utf-8")
            with patch(
                "pdf_foundry.parsing.pdf_parser.parse_pdf_to_markdown",
                return_value="# Heading\ncontent\n",
            ):
                changed_2, manifest_2 = parse_incremental_pdf_dir(
                    pdf_dir=pdf_dir, markdown_dir=md_dir, manifest_path=manifest_path
                )
                self.assertEqual(len(changed_2), 0)
                self.assertEqual(manifest, manifest_2)


if __name__ == "__main__":
    unittest.main()

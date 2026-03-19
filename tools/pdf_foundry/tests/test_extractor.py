from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from pdf_foundry.extraction.extractor import (
    SignatureExtractor,
    append_signatures_jsonl,
    load_signatures_jsonl,
    split_markdown_chunks,
)


class ExtractorTests(unittest.TestCase):
    def test_split_markdown_chunks_detects_findings(self) -> None:
        markdown = "# Finding 1\nA\n\n## Finding 2\nB"
        chunks = split_markdown_chunks(markdown)
        self.assertEqual(len(chunks), 2)
        self.assertEqual(chunks[0].index, 1)
        self.assertIn("Finding 1", chunks[0].chunk)

    def test_extractor_emits_schema_valid_signature(self) -> None:
        repo_root = Path(__file__).resolve().parents[3]
        schema_path = repo_root / "docs" / "memory-block-vulnerability-signature-schema.json"
        extractor = SignatureExtractor(schema_path=schema_path)
        signatures = extractor.extract_from_markdown(
            markdown="# Finding 9\nNonce reset after restart allows collisions.",
            report="Sample Audit",
            pdf_path="reports/sample.pdf",
        )
        self.assertEqual(len(signatures), 1)
        signature = signatures[0]
        self.assertEqual(signature.source.kind, "audit_report")
        self.assertIsInstance(signature.invariants.kani_hint, (str, type(None)))
        self.assertIsInstance(signature.evidence.section_title, (str, type(None)))
        self.assertTrue(signature.embedding_text)

    def test_append_signatures_jsonl_deduplicates_by_content_hash(self) -> None:
        repo_root = Path(__file__).resolve().parents[3]
        schema_path = repo_root / "docs" / "memory-block-vulnerability-signature-schema.json"
        extractor = SignatureExtractor(schema_path=schema_path)
        signatures = extractor.extract_from_markdown(
            markdown="# Finding 1\nState reset vulnerability.",
            report="Sample Audit",
            pdf_path="reports/sample.pdf",
        )
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "signatures.jsonl"
            first = append_signatures_jsonl(path, signatures)
            second = append_signatures_jsonl(path, signatures)
            loaded = load_signatures_jsonl(path)
            self.assertEqual(first, 1)
            self.assertEqual(second, 0)
            self.assertEqual(len(loaded), 1)


if __name__ == "__main__":
    unittest.main()

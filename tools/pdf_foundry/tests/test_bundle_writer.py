from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import msgpack

from pdf_foundry.bundling.bundle_writer import HEADER_LEN, parse_header, write_knowledge_bin
from pdf_foundry.schema.signature import (
    EmbeddingMetadata,
    Evidence,
    ExtractionMetadata,
    Invariants,
    Remediation,
    SignatureSource,
    VulnerabilityDetails,
    VulnerabilitySignature,
)


def _signature(signature_id: str) -> VulnerabilitySignature:
    return VulnerabilitySignature(
        id=signature_id,
        source=SignatureSource(
            report="Sample Report",
            pdf_path="reports/sample.pdf",
            page_range=(1, 2),
            kind="audit_report",
        ),
        vulnerability=VulnerabilityDetails(
            title="Nonce reuse",
            severity="high",
            category="nonce-reuse",
            description="Counter resets on restart",
            vulnerable_pattern="let nonce = counter.fetch_add(1, Ordering::Relaxed);",
            root_cause="State reset allows nonce reuse",
        ),
        remediation=Remediation(
            description="Persist counter state",
            code_pattern="let nonce = derive_nonce(session_id, counter);",
        ),
        invariants=Invariants(
            natural_language="Distinct encrypt calls must not reuse nonce.",
            kani_hint="kani::assume(a != b);",
        ),
        evidence=Evidence(excerpt="Counter resets to zero after restart.", section_title="Finding 1"),
        extraction=ExtractionMetadata(
            confidence="medium",
            review_status="unreviewed",
            embedding_text_version="v1",
        ),
        tags=["nonce", "aead"],
        embedding_text="Nonce reuse. State reset allows nonce reuse. nonce aead",
    )


class BundleWriterTests(unittest.TestCase):
    def test_write_knowledge_bin_layout(self) -> None:
        repo_root = Path(__file__).resolve().parents[3]
        artifact_schema = repo_root / "docs" / "memory-block-artifact-metadata-schema.json"
        signature = _signature("SIG-1")
        embedding = EmbeddingMetadata(
            provider="onnx",
            model="all-MiniLM-L6-v2",
            dimensions=3,
            distance_metric="cosine",
            l2_normalized=True,
            embedding_text_version="v1",
        )

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "knowledge.bin"
            write_knowledge_bin(
                output_path=path,
                embedding=embedding,
                signatures=[signature],
                vectors=[[1.0, 0.0, 0.0]],
                artifact_schema_path=artifact_schema,
            )

            header = parse_header(path)
            self.assertEqual(header["provider"], "onnx")
            self.assertEqual(header["dimensions"], 3)
            self.assertEqual(header["signature_count"], 1)

            raw = path.read_bytes()
            metadata_start = header["metadata_offset"]
            metadata_end = metadata_start + header["metadata_size"]
            metadata = msgpack.unpackb(raw[metadata_start:metadata_end], raw=False)
            self.assertEqual(metadata["embedding"]["model"], "all-MiniLM-L6-v2")
            self.assertEqual(len(metadata["signatures"]), 1)
            self.assertEqual(raw[:4], b"CPKN")
            self.assertGreaterEqual(len(raw), HEADER_LEN)


if __name__ == "__main__":
    unittest.main()

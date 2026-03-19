from __future__ import annotations

import struct
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import msgpack

from pdf_foundry.schema.signature import (
    ArtifactMetadata,
    EmbeddingMetadata,
    VulnerabilitySignature,
    load_artifact_schema,
    validate_artifact_payload,
)

MAGIC = b"CPKN"
FORMAT_VERSION_V1 = 1
HEADER_LEN = 0x100


def build_artifact_payload(
    *,
    embedding: EmbeddingMetadata,
    signatures: list[VulnerabilitySignature],
    schema_version: int = 1,
    generated_at: str | None = None,
) -> ArtifactMetadata:
    if generated_at is None:
        generated_at = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    return ArtifactMetadata(
        schema_version=schema_version,
        generated_at=generated_at,
        embedding=embedding,
        signatures=signatures,
    )


def write_knowledge_bin(
    *,
    output_path: Path,
    embedding: EmbeddingMetadata,
    signatures: list[VulnerabilitySignature],
    vectors: list[list[float]],
    artifact_schema_path: Path,
    schema_version: int = 1,
    generated_at: str | None = None,
) -> None:
    _validate_vector_shape(vectors=vectors, expected_rows=len(signatures), dimensions=embedding.dimensions)

    metadata = build_artifact_payload(
        embedding=embedding,
        signatures=signatures,
        schema_version=schema_version,
        generated_at=generated_at,
    )
    metadata_payload = metadata.model_dump(mode="json")
    artifact_schema = load_artifact_schema(artifact_schema_path)
    validate_artifact_payload(metadata_payload, artifact_schema)

    metadata_blob = msgpack.packb(metadata_payload, use_bin_type=True)
    vector_blob = _vectors_to_bytes(vectors)

    vector_offset = HEADER_LEN
    vector_size = len(vector_blob)
    metadata_offset = vector_offset + vector_size
    metadata_size = len(metadata_blob)

    header = bytearray(HEADER_LEN)
    header[0:4] = MAGIC
    header[0x04:0x08] = FORMAT_VERSION_V1.to_bytes(4, "little")
    _write_null_padded(header, 0x08, 32, embedding.provider)
    _write_null_padded(header, 0x28, 64, embedding.model)
    header[0x68:0x6C] = int(embedding.dimensions).to_bytes(4, "little")
    header[0x6C:0x70] = len(signatures).to_bytes(4, "little")
    header[0x70:0x78] = vector_offset.to_bytes(8, "little")
    header[0x78:0x80] = vector_size.to_bytes(8, "little")
    header[0x80:0x88] = metadata_offset.to_bytes(8, "little")
    header[0x88:0x90] = metadata_size.to_bytes(8, "little")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("wb") as handle:
        handle.write(header)
        handle.write(vector_blob)
        handle.write(metadata_blob)


def parse_header(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        header = handle.read(HEADER_LEN)
    if len(header) < HEADER_LEN:
        raise ValueError(f"header too short: expected {HEADER_LEN} bytes, got {len(header)}")
    if header[0:4] != MAGIC:
        raise ValueError("invalid magic")
    return {
        "version": int.from_bytes(header[0x04:0x08], "little"),
        "provider": _read_null_padded(header, 0x08, 32),
        "model": _read_null_padded(header, 0x28, 64),
        "dimensions": int.from_bytes(header[0x68:0x6C], "little"),
        "signature_count": int.from_bytes(header[0x6C:0x70], "little"),
        "vector_offset": int.from_bytes(header[0x70:0x78], "little"),
        "vector_size": int.from_bytes(header[0x78:0x80], "little"),
        "metadata_offset": int.from_bytes(header[0x80:0x88], "little"),
        "metadata_size": int.from_bytes(header[0x88:0x90], "little"),
    }


def _validate_vector_shape(*, vectors: list[list[float]], expected_rows: int, dimensions: int) -> None:
    if expected_rows == 0:
        raise ValueError("cannot write empty corpus; at least one signature is required")
    if len(vectors) != expected_rows:
        raise ValueError(
            f"vector count mismatch: expected {expected_rows}, got {len(vectors)}"
        )
    if dimensions <= 0:
        raise ValueError("embedding dimensions must be > 0")
    for index, vector in enumerate(vectors):
        if len(vector) != dimensions:
            raise ValueError(
                f"vector dimension mismatch at row {index}: expected {dimensions}, got {len(vector)}"
            )


def _vectors_to_bytes(vectors: list[list[float]]) -> bytes:
    blob = bytearray()
    for row in vectors:
        for value in row:
            blob.extend(struct.pack("<f", float(value)))
    return bytes(blob)


def _write_null_padded(buffer: bytearray, offset: int, length: int, value: str) -> None:
    encoded = value.encode("utf-8")[:length]
    buffer[offset : offset + length] = b"\x00" * length
    buffer[offset : offset + len(encoded)] = encoded


def _read_null_padded(buffer: bytes, offset: int, length: int) -> str:
    raw = buffer[offset : offset + length]
    end = raw.find(b"\x00")
    if end == -1:
        end = len(raw)
    return raw[:end].decode("utf-8", errors="replace").strip()

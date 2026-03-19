from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any, Literal

from jsonschema import Draft7Validator
from pydantic import BaseModel, Field


class SignatureSource(BaseModel):
    report: str
    pdf_path: str
    page_range: tuple[int, int]
    kind: str


class VulnerabilityDetails(BaseModel):
    title: str
    severity: str
    category: str
    description: str
    vulnerable_pattern: str
    root_cause: str


class Remediation(BaseModel):
    description: str
    code_pattern: str


class Invariants(BaseModel):
    natural_language: str
    kani_hint: str | None = None


class Evidence(BaseModel):
    excerpt: str
    section_title: str | None = None


class ExtractionMetadata(BaseModel):
    confidence: str
    review_status: str
    embedding_text_version: str


class VulnerabilitySignature(BaseModel):
    id: str
    source: SignatureSource
    vulnerability: VulnerabilityDetails
    remediation: Remediation
    invariants: Invariants
    evidence: Evidence
    extraction: ExtractionMetadata
    tags: list[str] = Field(default_factory=list)
    embedding_text: str


class EmbeddingMetadata(BaseModel):
    provider: str
    model: str
    dimensions: int
    distance_metric: Literal["cosine"]
    l2_normalized: bool
    embedding_text_version: str


class ArtifactMetadata(BaseModel):
    schema_version: int
    generated_at: str
    embedding: EmbeddingMetadata
    signatures: list[VulnerabilitySignature]


def load_signature_schema(path: Path) -> dict[str, Any]:
    return _load_json(path)


def load_artifact_schema(path: Path) -> dict[str, Any]:
    return _load_json(path)


def _load_json(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"schema at {path} is not a JSON object")
    return payload


def validate_signature_payload(payload: dict[str, Any], schema: dict[str, Any]) -> None:
    Draft7Validator(schema).validate(payload)
    VulnerabilitySignature.model_validate(payload)


def validate_artifact_payload(payload: dict[str, Any], schema: dict[str, Any]) -> None:
    Draft7Validator(schema).validate(payload)
    ArtifactMetadata.model_validate(payload)


def signature_content_hash(signature: VulnerabilitySignature) -> str:
    canonical = signature.model_dump(mode="json")
    encoded = json.dumps(canonical, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()

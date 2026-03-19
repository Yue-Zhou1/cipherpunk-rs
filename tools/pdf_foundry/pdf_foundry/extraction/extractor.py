from __future__ import annotations

import hashlib
import json
import logging
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import httpx

from pdf_foundry.extraction.prompts import (
    EMBEDDING_TEXT_VERSION,
    EXTRACTION_SYSTEM_PROMPT,
    EXTRACTION_USER_PROMPT_TEMPLATE,
    render_embedding_text,
)
from pdf_foundry.schema.signature import (
    VulnerabilitySignature,
    load_signature_schema,
    signature_content_hash,
    validate_signature_payload,
)

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class ExtractedChunk:
    index: int
    chunk: str


class OpenAiCompatibleExtractorClient:
    def __init__(
        self,
        *,
        base_url: str,
        model: str,
        api_key: str | None = None,
        timeout_secs: float = 60.0,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._model = model
        self._api_key = api_key
        self._timeout_secs = timeout_secs

    def extract(
        self,
        *,
        chunk: str,
        chunk_index: int,
        report: str,
        pdf_path: str,
        schema: dict[str, Any],
    ) -> dict[str, Any]:
        payload = {
            "model": self._model,
            "messages": [
                {"role": "system", "content": EXTRACTION_SYSTEM_PROMPT},
                {
                    "role": "user",
                    "content": EXTRACTION_USER_PROMPT_TEMPLATE.format(
                        schema_json=json.dumps(schema, indent=2, sort_keys=True),
                        report=report,
                        pdf_path=pdf_path,
                        chunk_index=chunk_index,
                        chunk=chunk,
                    ),
                },
            ],
            "temperature": 0,
            "response_format": {"type": "json_object"},
        }
        headers: dict[str, str] = {}
        if self._api_key:
            headers["Authorization"] = f"Bearer {self._api_key}"

        with httpx.Client(timeout=self._timeout_secs) as client:
            response = client.post(
                f"{self._base_url}/v1/chat/completions",
                headers=headers,
                json=payload,
            )
        response.raise_for_status()
        result = response.json()
        content = result["choices"][0]["message"]["content"]
        if isinstance(content, list):
            content = "".join(
                part.get("text", "") for part in content if isinstance(part, dict)
            )
        if not isinstance(content, str):
            raise ValueError("LLM response did not contain string JSON content")
        return json.loads(content)


class SignatureExtractor:
    def __init__(
        self,
        *,
        schema_path: Path,
        llm_client: OpenAiCompatibleExtractorClient | None = None,
        embedding_text_version: str = EMBEDDING_TEXT_VERSION,
    ) -> None:
        self._schema = load_signature_schema(schema_path)
        self._llm_client = llm_client
        self._embedding_text_version = embedding_text_version

    def extract_from_markdown(
        self, *, markdown: str, report: str, pdf_path: str, kind: str = "audit_report"
    ) -> list[VulnerabilitySignature]:
        signatures: list[VulnerabilitySignature] = []
        seen_hashes = set[str]()
        chunks = split_markdown_chunks(markdown)
        for extracted in chunks:
            raw = self._extract_chunk(
                chunk=extracted.chunk,
                chunk_index=extracted.index,
                report=report,
                pdf_path=pdf_path,
                kind=kind,
            )
            raw["embedding_text"] = render_embedding_text(
                raw["vulnerability"]["title"],
                raw["vulnerability"]["root_cause"],
                list(raw.get("tags", [])),
            )
            raw["extraction"]["embedding_text_version"] = self._embedding_text_version
            validate_signature_payload(raw, self._schema)
            signature = VulnerabilitySignature.model_validate(raw)
            content_hash = signature_content_hash(signature)
            if content_hash in seen_hashes:
                continue
            seen_hashes.add(content_hash)
            signatures.append(signature)
        return signatures

    def _extract_chunk(
        self,
        *,
        chunk: str,
        chunk_index: int,
        report: str,
        pdf_path: str,
        kind: str,
    ) -> dict[str, Any]:
        if self._llm_client is not None:
            payload = self._llm_client.extract(
                chunk=chunk,
                chunk_index=chunk_index,
                report=report,
                pdf_path=pdf_path,
                schema=self._schema,
            )
            return payload
        if chunk_index == 1:
            logger.warning(
                "pdf_foundry extraction is using heuristic fallback (no LLM extractor configured); "
                "generated signatures are low-confidence and should be reviewed manually"
            )
        return _heuristic_extract(
            chunk=chunk,
            chunk_index=chunk_index,
            report=report,
            pdf_path=pdf_path,
            kind=kind,
        )


def split_markdown_chunks(markdown: str, max_chars: int = 5000) -> list[ExtractedChunk]:
    boundaries = re.compile(r"^(?:#{1,3}\s+.+|finding\s+\d+.*)$", re.IGNORECASE)
    chunks: list[str] = []
    current: list[str] = []

    for line in markdown.splitlines():
        if boundaries.match(line.strip()) and current:
            chunks.append("\n".join(current).strip())
            current = [line]
            continue
        current.append(line)
        if sum(len(entry) for entry in current) > max_chars:
            chunks.append("\n".join(current).strip())
            current = []

    if current:
        chunks.append("\n".join(current).strip())

    compact = [chunk for chunk in chunks if chunk]
    if not compact and markdown.strip():
        compact = [markdown.strip()]
    return [ExtractedChunk(index=i + 1, chunk=chunk) for i, chunk in enumerate(compact)]


def load_signatures_jsonl(path: Path) -> list[VulnerabilitySignature]:
    if not path.exists():
        return []
    signatures: list[VulnerabilitySignature] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        signatures.append(VulnerabilitySignature.model_validate_json(line))
    return signatures


def append_signatures_jsonl(path: Path, new_signatures: list[VulnerabilitySignature]) -> int:
    existing = load_signatures_jsonl(path)
    seen_hashes = {signature_content_hash(entry) for entry in existing}
    accepted: list[VulnerabilitySignature] = []
    for signature in new_signatures:
        digest = signature_content_hash(signature)
        if digest in seen_hashes:
            continue
        accepted.append(signature)
        seen_hashes.add(digest)

    if not accepted:
        return 0

    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        for signature in accepted:
            handle.write(signature.model_dump_json())
            handle.write("\n")
    return len(accepted)


def _heuristic_extract(
    *, chunk: str, chunk_index: int, report: str, pdf_path: str, kind: str
) -> dict[str, Any]:
    section_title = _first_heading(chunk)
    title = section_title or _first_sentence(chunk) or "Security finding"
    severity = _infer_severity(chunk)
    category = _infer_category(chunk)
    vulnerable_pattern = _first_code_line(chunk) or "unknown_pattern()"
    root_cause = _infer_root_cause(chunk, category)
    tags = _infer_tags(chunk, category)

    signature_seed = hashlib.sha256(
        f"{report}|{pdf_path}|{chunk_index}|{title}".encode("utf-8")
    ).hexdigest()[:10]
    signature_id = f"{_slug(report)}-finding-{chunk_index}-{signature_seed}"

    return {
        "id": signature_id,
        "source": {
            "report": report,
            "pdf_path": pdf_path,
            "page_range": [1, 1],
            "kind": kind,
        },
        "vulnerability": {
            "title": title,
            "severity": severity,
            "category": category,
            "description": _truncate_text(chunk, 260),
            "vulnerable_pattern": vulnerable_pattern,
            "root_cause": root_cause,
        },
        "remediation": {
            "description": "Introduce invariant-preserving checks and explicit state handling.",
            "code_pattern": "apply_defensive_checks_and_state_validation();",
        },
        "invariants": {
            "natural_language": f"{title}: execution should preserve the intended safety invariant.",
            "kani_hint": _kani_hint_from_tags(tags),
        },
        "evidence": {
            "excerpt": _truncate_text(chunk, 220),
            "section_title": section_title,
        },
        "extraction": {
            "confidence": "low",
            "review_status": "unreviewed",
            "embedding_text_version": EMBEDDING_TEXT_VERSION,
        },
        "tags": tags,
        "embedding_text": "",
    }


def _first_heading(text: str) -> str | None:
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("#"):
            return stripped.lstrip("#").strip() or None
        if re.match(r"^finding\s+\d+", stripped, re.IGNORECASE):
            return stripped
    return None


def _first_sentence(text: str) -> str:
    clean = re.sub(r"\s+", " ", text).strip()
    if not clean:
        return ""
    dot = clean.find(".")
    if dot == -1:
        return _truncate_text(clean, 80)
    return _truncate_text(clean[: dot + 1], 80)


def _infer_severity(text: str) -> str:
    lowered = text.lower()
    for level in ("critical", "high", "medium", "low"):
        if level in lowered:
            return level
    return "medium"


def _infer_category(text: str) -> str:
    lowered = text.lower()
    if "nonce" in lowered:
        return "nonce-reuse"
    if "overflow" in lowered:
        return "overflow"
    if "signature" in lowered:
        return "signature-validation"
    if "panic" in lowered or "unwrap" in lowered:
        return "panic"
    return "crypto"


def _first_code_line(text: str) -> str | None:
    block = re.search(r"```(?:[a-zA-Z0-9_+-]*)\n(.*?)```", text, re.DOTALL)
    if not block:
        return None
    for line in block.group(1).splitlines():
        stripped = line.strip()
        if stripped:
            return _truncate_text(stripped, 120)
    return None


def _infer_root_cause(text: str, category: str) -> str:
    lowered = text.lower()
    if "restart" in lowered or "reset" in lowered:
        return "State reset path invalidates security assumptions."
    if category == "nonce-reuse":
        return "Nonce lifecycle is not guaranteed to be unique."
    if category == "overflow":
        return "Arithmetic boundary checks are missing."
    return "Validation and invariants are underspecified for this code path."


def _infer_tags(text: str, category: str) -> list[str]:
    lowered = text.lower()
    tags = {category}
    for token in ["aead", "nonce", "signature", "overflow", "state", "panic", "unwrap"]:
        if token in lowered:
            tags.add(token)
    return sorted(tags)


def _kani_hint_from_tags(tags: list[str]) -> str | None:
    if "nonce" in tags:
        return "kani::assume(nonce_a != nonce_b);"
    if "overflow" in tags:
        return "kani::assume(input <= u64::MAX / 2);"
    return None


def _truncate_text(value: str, limit: int) -> str:
    compact = re.sub(r"\s+", " ", value).strip()
    if len(compact) <= limit:
        return compact
    return compact[:limit].rstrip() + "..."


def _slug(value: str) -> str:
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", value.lower()).strip("-")
    return slug or "report"

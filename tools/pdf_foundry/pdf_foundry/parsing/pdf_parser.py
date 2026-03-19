from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class ParsedDocument:
    pdf_path: Path
    markdown_path: Path
    sha256: str
    markdown: str


def parse_pdf_to_markdown(pdf_path: Path) -> str:
    try:
        import pymupdf4llm  # type: ignore
    except ImportError as exc:  # pragma: no cover - dependency guard
        raise RuntimeError(
            "pymupdf4llm is required for PDF parsing. Install with `pip install pymupdf4llm`."
        ) from exc

    markdown = pymupdf4llm.to_markdown(str(pdf_path))
    return normalize_markdown(markdown)


def normalize_markdown(markdown: str) -> str:
    lines = [line.rstrip() for line in markdown.splitlines()]
    lines = _strip_repeated_headers(lines)
    joined = "\n".join(lines)
    joined = re.sub(r"\n{3,}", "\n\n", joined)
    return joined.strip() + "\n"


def _strip_repeated_headers(lines: list[str]) -> list[str]:
    frequency: dict[str, int] = {}
    for raw in lines:
        line = raw.strip()
        if not line:
            continue
        frequency[line] = frequency.get(line, 0) + 1

    stripped: list[str] = []
    for raw in lines:
        line = raw.strip()
        if line and frequency.get(line, 0) > 8 and len(line) < 120:
            continue
        stripped.append(raw)
    return stripped


def parse_incremental_pdf_dir(
    pdf_dir: Path, markdown_dir: Path, manifest_path: Path
) -> tuple[list[ParsedDocument], dict[str, str]]:
    pdf_dir = pdf_dir.resolve()
    markdown_dir.mkdir(parents=True, exist_ok=True)

    existing_manifest = load_manifest(manifest_path)
    next_manifest = dict(existing_manifest)
    changed: list[ParsedDocument] = []

    for pdf_path in sorted(pdf_dir.rglob("*.pdf")):
        rel_pdf = pdf_path.relative_to(pdf_dir).as_posix()
        digest = file_sha256(pdf_path)
        if existing_manifest.get(rel_pdf) == digest:
            continue

        markdown = parse_pdf_to_markdown(pdf_path)
        safe_name = rel_pdf.replace("/", "__")
        markdown_path = markdown_dir / f"{safe_name}.md"
        markdown_path.parent.mkdir(parents=True, exist_ok=True)
        markdown_path.write_text(markdown, encoding="utf-8")

        changed.append(
            ParsedDocument(
                pdf_path=pdf_path,
                markdown_path=markdown_path,
                sha256=digest,
                markdown=markdown,
            )
        )
        next_manifest[rel_pdf] = digest

    return changed, next_manifest


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def load_manifest(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        return {}
    manifest: dict[str, str] = {}
    for key, value in payload.items():
        if isinstance(key, str) and isinstance(value, str):
            manifest[key] = value
    return manifest


def save_manifest(path: Path, manifest: dict[str, str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload: dict[str, Any] = dict(sorted(manifest.items()))
    path.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")

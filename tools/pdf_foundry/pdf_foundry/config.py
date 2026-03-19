from __future__ import annotations

import os
import warnings
from dataclasses import dataclass
from pathlib import Path
from typing import Literal


def _env_or(key: str, default: str) -> str:
    value = os.getenv(key, "").strip()
    return value if value else default


def _env_bool(key: str, default: bool) -> bool:
    value = os.getenv(key, "").strip().lower()
    if not value:
        return default
    return value in {"1", "true", "yes", "on"}


def _env_u32(key: str, default: int) -> int:
    raw = os.getenv(key, "").strip()
    if not raw:
        return default
    try:
        parsed = int(raw)
    except ValueError:
        return default
    return parsed if parsed > 0 else default


@dataclass(frozen=True)
class EmbeddingSettings:
    provider: str
    model: str
    dimensions: int
    distance_metric: Literal["cosine"]
    l2_normalized: bool
    embedding_text_version: str
    onnx_model_path: Path
    onnx_tokenizer_path: Path | None
    base_url: str | None
    api_key: str | None


@dataclass(frozen=True)
class FoundryPaths:
    repo_root: Path
    root: Path
    models_dir: Path
    data_dir: Path
    markdown_dir: Path
    signatures_path: Path
    manifest_path: Path
    output_dir: Path
    bundle_path: Path
    vulnerability_schema_path: Path
    artifact_schema_path: Path

@dataclass(frozen=True)
class FoundryConfig:
    paths: FoundryPaths
    embedding: EmbeddingSettings


def _default_repo_root(foundry_root: Path) -> Path:
    # tools/pdf_foundry -> repo root is two levels up
    return foundry_root.parent.parent


def load_config(root: Path | None = None) -> FoundryConfig:
    if root is None:
        root = Path(__file__).resolve().parents[1]
    repo_root = _default_repo_root(root)

    paths = FoundryPaths(
        repo_root=repo_root,
        root=root,
        models_dir=root / "models",
        data_dir=root / "data",
        markdown_dir=root / "data" / "markdown",
        signatures_path=root / "data" / "signatures.jsonl",
        manifest_path=root / "data" / "manifest.json",
        output_dir=root / "output",
        bundle_path=root / "output" / "knowledge.bin",
        vulnerability_schema_path=repo_root
        / "docs"
        / "memory-block-vulnerability-signature-schema.json",
        artifact_schema_path=repo_root
        / "docs"
        / "memory-block-artifact-metadata-schema.json",
    )
    provider = _env_or("KNOWLEDGE_EMBEDDING_PROVIDER", "onnx").lower()
    default_base_url = {
        "openai": "https://api.openai.com",
        "ollama": "http://localhost:11434",
        "http": "http://localhost:11434",
    }.get(provider)
    base_url = _env_or("KNOWLEDGE_EMBEDDING_BASE_URL", default_base_url or "").strip()
    if not base_url:
        base_url = None
    api_key = _env_or("KNOWLEDGE_EMBEDDING_API_KEY", _env_or("LLM_API_KEY", "")).strip()
    if not api_key:
        api_key = None

    default_model_path = paths.models_dir / "all-MiniLM-L6-v2.onnx"
    tokenizer_default = paths.models_dir / "all-MiniLM-L6-v2-tokenizer.json"
    tokenizer_raw = _env_or("KNOWLEDGE_ONNX_TOKENIZER_PATH", "")
    tokenizer_path = Path(tokenizer_raw) if tokenizer_raw else tokenizer_default
    if not tokenizer_path.exists():
        tokenizer_path = None

    distance_metric = _env_or("KNOWLEDGE_EMBEDDING_DISTANCE", "cosine").lower()
    if distance_metric != "cosine":
        warnings.warn(
            f"unsupported KNOWLEDGE_EMBEDDING_DISTANCE={distance_metric!r}; falling back to 'cosine'",
            stacklevel=2,
        )
        distance_metric = "cosine"

    embedding = EmbeddingSettings(
        provider=provider,
        model=_env_or("KNOWLEDGE_EMBEDDING_MODEL", "all-MiniLM-L6-v2"),
        dimensions=_env_u32("KNOWLEDGE_EMBEDDING_DIMENSIONS", 384),
        distance_metric=distance_metric,
        l2_normalized=_env_bool("KNOWLEDGE_EMBEDDING_L2_NORMALIZED", True),
        embedding_text_version=_env_or("KNOWLEDGE_EMBEDDING_TEXT_VERSION", "v1"),
        onnx_model_path=Path(_env_or("KNOWLEDGE_ONNX_MODEL_PATH", str(default_model_path))),
        onnx_tokenizer_path=tokenizer_path,
        base_url=base_url,
        api_key=api_key,
    )
    return FoundryConfig(paths=paths, embedding=embedding)

from __future__ import annotations

import json
import math
import os
from pathlib import Path

import click

from pdf_foundry.bundling.bundle_writer import parse_header, write_knowledge_bin
from pdf_foundry.config import FoundryConfig, load_config
from pdf_foundry.embedding.api_embedder import ApiEmbedder
from pdf_foundry.embedding.onnx_embedder import OnnxEmbedder
from pdf_foundry.extraction.extractor import (
    OpenAiCompatibleExtractorClient,
    SignatureExtractor,
    append_signatures_jsonl,
    load_signatures_jsonl,
)
from pdf_foundry.parsing.pdf_parser import parse_incremental_pdf_dir, save_manifest
from pdf_foundry.schema.signature import EmbeddingMetadata, VulnerabilitySignature


@click.group()
def cli() -> None:
    """PDF foundry CLI."""


@cli.command("ingest")
@click.option("--pdf-dir", required=True, type=click.Path(exists=True, file_okay=False, path_type=Path))
@click.option("--kind", default="audit_report", show_default=True, help="Source kind tag for extracted signatures.")
def ingest(pdf_dir: Path, kind: str) -> None:
    config = load_config()
    _ensure_write_dirs(config, include_markdown=True)
    changed, next_manifest = parse_incremental_pdf_dir(
        pdf_dir=pdf_dir,
        markdown_dir=config.paths.markdown_dir,
        manifest_path=config.paths.manifest_path,
    )

    extractor = _build_extractor(config)
    extracted_count = 0
    for document in changed:
        report = document.pdf_path.stem.replace("_", " ").strip() or document.pdf_path.stem
        signatures = extractor.extract_from_markdown(
            markdown=document.markdown,
            report=report,
            pdf_path=document.pdf_path.as_posix(),
            kind=kind,
        )
        extracted_count += append_signatures_jsonl(config.paths.signatures_path, signatures)

    save_manifest(config.paths.manifest_path, next_manifest)
    signatures = load_signatures_jsonl(config.paths.signatures_path)
    _rebuild_bundle(config, signatures)

    click.echo(
        f"ingest complete: changed_pdfs={len(changed)} new_signatures={extracted_count} "
        f"total_signatures={len(signatures)} bundle={config.paths.bundle_path}"
    )


@cli.command("rebuild-bundle")
def rebuild_bundle() -> None:
    config = load_config()
    _ensure_write_dirs(config, include_markdown=False)
    signatures = load_signatures_jsonl(config.paths.signatures_path)
    _rebuild_bundle(config, signatures)
    click.echo(f"bundle rebuilt: signatures={len(signatures)} path={config.paths.bundle_path}")


@cli.command("export")
@click.option("--format", "export_format", type=click.Choice(["json", "jsonl"]), default="json", show_default=True)
@click.option("--output", required=True, type=click.Path(dir_okay=False, path_type=Path))
def export(export_format: str, output: Path) -> None:
    config = load_config()
    signatures = load_signatures_jsonl(config.paths.signatures_path)
    output.parent.mkdir(parents=True, exist_ok=True)

    if export_format == "json":
        payload = [entry.model_dump(mode="json") for entry in signatures]
        output.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    else:
        with output.open("w", encoding="utf-8") as handle:
            for signature in signatures:
                handle.write(signature.model_dump_json())
                handle.write("\n")
    click.echo(f"exported {len(signatures)} signatures to {output}")


@cli.command("info")
def info() -> None:
    config = load_config()
    signatures = load_signatures_jsonl(config.paths.signatures_path)
    click.echo(f"signatures_path={config.paths.signatures_path}")
    click.echo(f"signature_count={len(signatures)}")
    click.echo(f"bundle_path={config.paths.bundle_path}")
    click.echo(
        "embedding="
        + json.dumps(
            {
                "provider": config.embedding.provider,
                "model": config.embedding.model,
                "dimensions": config.embedding.dimensions,
                "distance_metric": config.embedding.distance_metric,
                "l2_normalized": config.embedding.l2_normalized,
                "embedding_text_version": config.embedding.embedding_text_version,
            },
            sort_keys=True,
        )
    )
    if config.paths.bundle_path.exists():
        header = parse_header(config.paths.bundle_path)
        click.echo("bundle_header=" + json.dumps(header, sort_keys=True))


def _build_extractor(config: FoundryConfig) -> SignatureExtractor:
    llm_client: OpenAiCompatibleExtractorClient | None = None
    extraction_base_url = os.getenv("PDF_FOUNDRY_EXTRACTOR_BASE_URL", "").strip()
    extraction_model = os.getenv("PDF_FOUNDRY_EXTRACTOR_MODEL", "").strip()
    if extraction_base_url and extraction_model:
        llm_client = OpenAiCompatibleExtractorClient(
            base_url=extraction_base_url,
            model=extraction_model,
            api_key=os.getenv("PDF_FOUNDRY_EXTRACTOR_API_KEY", "").strip() or None,
        )
    return SignatureExtractor(
        schema_path=config.paths.vulnerability_schema_path,
        llm_client=llm_client,
        embedding_text_version=config.embedding.embedding_text_version,
    )


def _rebuild_bundle(config: FoundryConfig, signatures: list[VulnerabilitySignature]) -> None:
    if not signatures:
        raise click.ClickException(
            f"no signatures found in {config.paths.signatures_path}; run ingest first"
        )

    embedding_meta = EmbeddingMetadata(
        provider=config.embedding.provider,
        model=config.embedding.model,
        dimensions=config.embedding.dimensions,
        distance_metric=config.embedding.distance_metric,
        l2_normalized=config.embedding.l2_normalized,
        embedding_text_version=config.embedding.embedding_text_version,
    )

    texts = [entry.embedding_text for entry in signatures]
    # Normalization policy is owned by the bundle build path so all providers
    # follow one consistent rule before serialization.
    vectors = _embed_texts(config, texts)
    if config.embedding.l2_normalized:
        vectors = [_l2_normalize(vector) for vector in vectors]

    write_knowledge_bin(
        output_path=config.paths.bundle_path,
        embedding=embedding_meta,
        signatures=signatures,
        vectors=vectors,
        artifact_schema_path=config.paths.artifact_schema_path,
    )


def _embed_texts(config: FoundryConfig, texts: list[str]) -> list[list[float]]:
    provider = config.embedding.provider.lower()
    if provider in {"openai", "ollama", "http"}:
        if not config.embedding.base_url:
            raise click.ClickException(
                "KNOWLEDGE_EMBEDDING_BASE_URL is required for API embeddings"
            )
        embedder = ApiEmbedder(
            base_url=config.embedding.base_url,
            model=config.embedding.model,
            dimensions=config.embedding.dimensions,
            api_key=config.embedding.api_key,
        )
        return embedder.embed_texts(texts)

    if provider == "onnx":
        embedder = OnnxEmbedder(
            model_path=config.embedding.onnx_model_path,
            dimensions=config.embedding.dimensions,
            tokenizer_path=config.embedding.onnx_tokenizer_path,
        )
        return embedder.embed_texts(texts)

    raise click.ClickException(f"unsupported embedding provider: {config.embedding.provider}")


def _l2_normalize(values: list[float]) -> list[float]:
    norm = math.sqrt(sum(value * value for value in values))
    if norm <= 1e-12:
        return values
    return [value / norm for value in values]


def _ensure_write_dirs(config: FoundryConfig, *, include_markdown: bool) -> None:
    config.paths.models_dir.mkdir(parents=True, exist_ok=True)
    config.paths.data_dir.mkdir(parents=True, exist_ok=True)
    if include_markdown:
        config.paths.markdown_dir.mkdir(parents=True, exist_ok=True)
    config.paths.output_dir.mkdir(parents=True, exist_ok=True)


if __name__ == "__main__":
    cli()

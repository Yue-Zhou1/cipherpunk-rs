# PDF Foundry

Offline pipeline for extracting `VulnerabilitySignature` records from PDFs and
building a runtime `knowledge.bin` memory-block artifact.

## Commands

```bash
python -m pdf_foundry ingest --pdf-dir ./reports
python -m pdf_foundry rebuild-bundle
python -m pdf_foundry export --format json --output ./signatures.json
python -m pdf_foundry info
```

## Environment

Embedding settings mirror Rust runtime settings:

- `KNOWLEDGE_EMBEDDING_PROVIDER` (`onnx` default; `openai`/`ollama`/`http` optional)
- `KNOWLEDGE_EMBEDDING_MODEL` (`all-MiniLM-L6-v2` default)
- `KNOWLEDGE_EMBEDDING_DIMENSIONS` (`384` default)
- `KNOWLEDGE_EMBEDDING_DISTANCE` (`cosine` default)
- `KNOWLEDGE_EMBEDDING_L2_NORMALIZED` (`true` default)
- `KNOWLEDGE_EMBEDDING_TEXT_VERSION` (`v1` default)

Provider-specific optional settings:

- `KNOWLEDGE_ONNX_MODEL_PATH`
- `KNOWLEDGE_ONNX_TOKENIZER_PATH`
- `KNOWLEDGE_EMBEDDING_BASE_URL`
- `KNOWLEDGE_EMBEDDING_API_KEY` (falls back to `LLM_API_KEY`)

## Notes

- Raw PDFs and parsed markdown are local artifacts and should not be committed.
- Schema validation is performed against the Rust-owned snapshots in `docs/`:
  - `memory-block-vulnerability-signature-schema.json`
  - `memory-block-artifact-metadata-schema.json`

## Tests

Run tests from an editable install:

```bash
python3 -m venv .venv
.venv/bin/pip install -e .[dev]
.venv/bin/python -m unittest discover -s tests -v
```

EXTRACTION_SYSTEM_PROMPT = """You extract vulnerability signatures from security-audit markdown.
Return exactly one JSON object that follows the provided schema.
Do not add extra keys. Keep evidence excerpts short and factual."""

EXTRACTION_USER_PROMPT_TEMPLATE = """Schema:
{schema_json}

Report: {report}
PDF Path: {pdf_path}
Chunk Index: {chunk_index}

Markdown chunk:
{chunk}
"""

EMBEDDING_TEXT_VERSION = "v1"


def render_embedding_text(title: str, root_cause: str, tags: list[str]) -> str:
    tags_text = " ".join(tags)
    return f"{title}. {root_cause}. {tags_text}".strip()

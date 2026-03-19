from __future__ import annotations

import unittest
from pathlib import Path
from unittest.mock import patch

import httpx

from pdf_foundry.embedding.api_embedder import ApiEmbedder
from pdf_foundry.embedding.onnx_embedder import OnnxEmbedder


class EmbedderTests(unittest.TestCase):
    def test_onnx_embedder_hash_fallback_is_deterministic(self) -> None:
        embedder = OnnxEmbedder(model_path=Path("/does/not/exist.onnx"), dimensions=8)
        first = embedder.embed_texts(["nonce uniqueness invariant"])[0]
        second = embedder.embed_texts(["nonce uniqueness invariant"])[0]
        self.assertEqual(first, second)
        self.assertTrue(any(abs(value) > 1e-9 for value in first))

    def test_api_embedder_parses_openai_compatible_response(self) -> None:
        response = httpx.Response(
            200,
            json={
                "data": [
                    {"embedding": [0.1, 0.2, 0.3]},
                    {"embedding": [0.3, 0.2, 0.1]},
                ]
            },
            request=httpx.Request("POST", "https://example/v1/embeddings"),
        )
        with patch.object(httpx.Client, "post", return_value=response):
            embedder = ApiEmbedder(
                base_url="https://example",
                model="text-embedding-3-small",
                dimensions=3,
            )
            vectors = embedder.embed_texts(["a", "b"])
            self.assertEqual(len(vectors), 2)
            self.assertEqual(vectors[0], [0.1, 0.2, 0.3])


if __name__ == "__main__":
    unittest.main()

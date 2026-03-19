from __future__ import annotations

from dataclasses import dataclass

import httpx


@dataclass
class ApiEmbedder:
    base_url: str
    model: str
    dimensions: int
    api_key: str | None = None
    timeout_secs: float = 60.0

    def embed_texts(self, texts: list[str]) -> list[list[float]]:
        if not texts:
            return []
        payload = {
            "model": self.model,
            "input": texts,
            "encoding_format": "float",
        }
        headers: dict[str, str] = {}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"

        with httpx.Client(timeout=self.timeout_secs) as client:
            response = client.post(
                f"{self.base_url.rstrip('/')}/v1/embeddings",
                headers=headers,
                json=payload,
            )
        response.raise_for_status()
        body = response.json()
        data = body.get("data")
        if not isinstance(data, list):
            raise ValueError("embedding response missing `data` array")

        vectors: list[list[float]] = []
        for index, item in enumerate(data):
            if not isinstance(item, dict) or not isinstance(item.get("embedding"), list):
                raise ValueError(f"embedding response missing data[{index}].embedding")
            vector = [float(value) for value in item["embedding"]]
            if len(vector) != self.dimensions:
                raise ValueError(
                    f"embedding dimension mismatch: expected {self.dimensions}, got {len(vector)}"
                )
            vectors.append(vector)
        if len(vectors) != len(texts):
            raise ValueError(
                f"embedding response count mismatch: expected {len(texts)}, got {len(vectors)}"
            )
        return vectors

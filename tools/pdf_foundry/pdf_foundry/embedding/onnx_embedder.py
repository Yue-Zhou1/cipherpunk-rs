from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

@dataclass
class OnnxEmbedder:
    model_path: Path
    dimensions: int
    tokenizer_path: Path | None = None
    _session: Any | None = field(default=None, init=False, repr=False)
    _tokenizer: Any | None = field(default=None, init=False, repr=False)

    def __post_init__(self) -> None:
        self._try_init_runtime()

    def embed_texts(self, texts: list[str]) -> list[list[float]]:
        if not texts:
            return []
        if self._session is not None and self._tokenizer is not None:
            return [self._embed_with_runtime(text) for text in texts]
        return [_hash_projection(text, self.dimensions) for text in texts]

    def _try_init_runtime(self) -> None:
        if not self.model_path.exists():
            return
        if self.tokenizer_path is None or not self.tokenizer_path.exists():
            return
        try:
            import onnxruntime as ort  # type: ignore
            from tokenizers import Tokenizer  # type: ignore
        except ImportError:  # pragma: no cover - dependency guard
            return

        try:
            options = ort.SessionOptions()
            self._session = ort.InferenceSession(
                str(self.model_path),
                sess_options=options,
                providers=["CPUExecutionProvider"],
            )
            self._tokenizer = Tokenizer.from_file(str(self.tokenizer_path))
        except Exception:
            self._session = None
            self._tokenizer = None

    def _embed_with_runtime(self, text: str, max_length: int = 256) -> list[float]:
        import numpy as np  # type: ignore

        assert self._session is not None
        assert self._tokenizer is not None

        encoded = self._tokenizer.encode(text)
        ids = list(encoded.ids[:max_length])
        type_ids = list(encoded.type_ids[:max_length]) if encoded.type_ids else [0] * len(ids)
        attention = [1] * len(ids)

        pad_len = max(0, max_length - len(ids))
        ids.extend([0] * pad_len)
        type_ids.extend([0] * pad_len)
        attention.extend([0] * pad_len)

        inputs = {}
        input_names = {entry.name for entry in self._session.get_inputs()}
        if "input_ids" in input_names:
            inputs["input_ids"] = np.asarray([ids], dtype=np.int64)
        if "attention_mask" in input_names:
            inputs["attention_mask"] = np.asarray([attention], dtype=np.int64)
        if "token_type_ids" in input_names:
            inputs["token_type_ids"] = np.asarray([type_ids], dtype=np.int64)
        if not inputs:
            raise RuntimeError("ONNX model has no supported input names")

        outputs = self._session.run(None, inputs)
        if not outputs:
            raise RuntimeError("ONNX runtime returned no outputs")

        output = outputs[0]
        if getattr(output, "ndim", 0) == 2:
            vector = output[0].tolist()
        elif getattr(output, "ndim", 0) == 3:
            token_vectors = output[0]
            denom = max(1, sum(attention))
            pooled = token_vectors[: len(attention)]
            weighted = []
            hidden = pooled.shape[1]
            for dim in range(hidden):
                total = 0.0
                for idx in range(len(attention)):
                    total += float(pooled[idx][dim]) * float(attention[idx])
                weighted.append(total / float(denom))
            vector = weighted
        else:
            raise RuntimeError(f"unsupported ONNX output rank: {getattr(output, 'ndim', None)}")

        if len(vector) == self.dimensions:
            return [float(value) for value in vector]
        if len(vector) > self.dimensions:
            return [float(value) for value in vector[: self.dimensions]]
        padded = [float(value) for value in vector]
        padded.extend([0.0] * (self.dimensions - len(padded)))
        return padded


def _hash_projection(text: str, dimensions: int) -> list[float]:
    # Placeholder fallback used only when ONNX runtime assets are unavailable.
    # It is deterministic but not semantically portable across languages/models.
    # Production cross-language compatibility requires real ONNX embeddings.
    values = [0.0] * dimensions
    tokens = re.findall(r"[a-zA-Z0-9_]+", text.lower())
    if not tokens:
        return values
    for token in tokens:
        digest = hashlib.sha256(token.encode("utf-8")).digest()
        idx = int.from_bytes(digest[:4], "little") % dimensions
        sign = 1.0 if (digest[4] & 1) == 0 else -1.0
        weight = 1.0 + (digest[5] / 255.0)
        values[idx] += sign * weight
    return values

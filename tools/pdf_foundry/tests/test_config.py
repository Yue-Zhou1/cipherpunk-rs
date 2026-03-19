from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

from pdf_foundry.config import load_config


class ConfigTests(unittest.TestCase):
    def test_load_config_has_no_directory_creation_side_effects(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "pdf_foundry"
            root.mkdir(parents=True, exist_ok=True)
            config = load_config(root=root)
            self.assertFalse(config.paths.data_dir.exists())
            self.assertFalse(config.paths.markdown_dir.exists())
            self.assertFalse(config.paths.output_dir.exists())

    def test_invalid_distance_metric_falls_back_to_cosine(self) -> None:
        previous = os.environ.get("KNOWLEDGE_EMBEDDING_DISTANCE")
        os.environ["KNOWLEDGE_EMBEDDING_DISTANCE"] = "dot-product"
        try:
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp) / "pdf_foundry"
                root.mkdir(parents=True, exist_ok=True)
                config = load_config(root=root)
                self.assertEqual(config.embedding.distance_metric, "cosine")
        finally:
            if previous is None:
                os.environ.pop("KNOWLEDGE_EMBEDDING_DISTANCE", None)
            else:
                os.environ["KNOWLEDGE_EMBEDDING_DISTANCE"] = previous


if __name__ == "__main__":
    unittest.main()

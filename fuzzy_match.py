#!/usr/bin/env python3
"""
Fuzzy biomarker name matching using pre-computed embeddings.
Maps messy OCR/lab names to canonical standardized biomarker names.

Usage:
    from fuzzy_match import FuzzyMatcher
    matcher = FuzzyMatcher()
    result = matcher.match("Glycosylated Hemoglobin A1C")
    # → {"standardized_name": "hba1c", "similarity": 0.841, "matched_variant": "hemoglobin a1c"}
"""

import json, os, numpy as np
from pathlib import Path

DATA_DIR = Path(__file__).parent / "data" / "embeddings"


class FuzzyMatcher:
    def __init__(self, threshold: float = 0.7):
        self.threshold = threshold
        self.embeddings = np.load(DATA_DIR / "biomarker_embeddings.npy")
        with open(DATA_DIR / "index.json") as f:
            idx = json.load(f)
        self.texts = idx["texts"]
        self.standardized_names = idx["standardized_names"]
        self.model = None  # lazy load

    def _get_model(self):
        if self.model is None:
            from sentence_transformers import SentenceTransformer
            self.model = SentenceTransformer("all-MiniLM-L6-v2")
        return self.model

    def match(self, query: str, top_k: int = 1) -> list[dict]:
        model = self._get_model()
        q_emb = model.encode([query.lower()], normalize_embeddings=True)[0]
        sims = self.embeddings @ q_emb
        top_idx = np.argsort(sims)[-top_k:][::-1]
        results = []
        for i in top_idx:
            sim = float(sims[i])
            if sim >= self.threshold:
                results.append({
                    "standardized_name": self.standardized_names[i],
                    "similarity": round(sim, 3),
                    "matched_variant": self.texts[i]
                })
        return results

    def match_batch(self, queries: list[str]) -> list[dict]:
        model = self._get_model()
        q_embs = model.encode([q.lower() for q in queries], normalize_embeddings=True)
        results = []
        for q, q_emb in zip(queries, q_embs):
            sims = self.embeddings @ q_emb
            best_idx = np.argmax(sims)
            sim = float(sims[best_idx])
            results.append({
                "query": q,
                "standardized_name": self.standardized_names[best_idx] if sim >= self.threshold else None,
                "similarity": round(sim, 3),
                "matched_variant": self.texts[best_idx] if sim >= self.threshold else None,
                "confident": sim >= self.threshold
            })
        return results


if __name__ == "__main__":
    import sys
    matcher = FuzzyMatcher()
    if len(sys.argv) > 1:
        query = " ".join(sys.argv[1:])
        results = matcher.match(query, top_k=3)
        for r in results:
            print(f"  → {r['standardized_name']} (sim={r['similarity']}, matched '{r['matched_variant']}')")
    else:
        # Demo
        tests = [
            "Glycosylated Hemoglobin A1C",
            "LDL Cholesterol Direct",
            "Serum Creatinine",
            "Free Thyroxine",
            "C-Reactive Protein High Sensitivity",
            "Red Cell Count",
            "Gamma GT",
            "Blood Sugar Fasting",
        ]
        for t in tests:
            results = matcher.match(t)
            if results:
                r = results[0]
                print(f"  '{t}' → {r['standardized_name']} (sim={r['similarity']})")
            else:
                print(f"  '{t}' → NO MATCH (below threshold)")

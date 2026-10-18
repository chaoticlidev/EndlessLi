"""K-Means clustering over embedding vectors."""

from __future__ import annotations

from typing import Any

import numpy as np
from sklearn.cluster import KMeans


def kmeans(
    embeddings: list[list[float]],
    k: int,
    seed: int = 42,
) -> dict[str, Any]:
    if not embeddings:
        raise ValueError("embeddings must be non-empty")
    n = len(embeddings)
    if k < 1:
        raise ValueError("k must be positive")
    k = min(k, n)
    matrix = np.asarray(embeddings, dtype=float)
    model = KMeans(n_clusters=k, random_state=seed, n_init="auto")
    model.fit(matrix)
    labels = [int(value) for value in model.labels_]
    centroids = [[float(value) for value in row] for row in model.cluster_centers_]
    inertia = float(model.inertia_)
    return {"k": k, "labels": labels, "centroids": centroids, "inertia": inertia}

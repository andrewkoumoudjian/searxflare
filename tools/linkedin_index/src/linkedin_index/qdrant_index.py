from __future__ import annotations

import os
import json
from collections.abc import Iterable, Sequence
from typing import Any

# FastEmbed uses only public models here. Do not let a stale local Hugging Face
# credential turn a public model download into an authenticated 401.
os.environ.setdefault("HF_HUB_DISABLE_IMPLICIT_TOKEN", "1")

from fastembed import SparseTextEmbedding, TextEmbedding
from qdrant_client import QdrantClient, models

from .corpus import IndexDocument, point_id


COLLECTION_NAME = "linkedin-full"
DENSE_MODEL = "BAAI/bge-small-en-v1.5"
SPARSE_MODEL = "Qdrant/bm25"
DENSE_SIZE = 384
DENSE_TEXT_MAX_CHARS = 1000
DEFAULT_MAX_BATCH_PAYLOAD_BYTES = 24 * 1024 * 1024
PAYLOAD_INDEX_FIELDS = (
    "url",
    "aliases",
    "entity_type",
    "name",
    "title",
    "company",
    "email",
    "phone",
    "location",
    "industry",
    "source_files",
    "content_sources",
)


def _enum_value(value: Any) -> str:
    return str(getattr(value, "value", value)).lower()


class LinkedInIndex:
    def __init__(
        self,
        *,
        url: str = "http://127.0.0.1:6333",
        client: Any | None = None,
        dense_embedder: Any | None = None,
        sparse_embedder: Any | None = None,
    ) -> None:
        self.client = client or QdrantClient(url=url)
        self._dense_embedder = dense_embedder
        self._sparse_embedder = sparse_embedder

    @property
    def dense_embedder(self) -> Any:
        if self._dense_embedder is None:
            self._dense_embedder = TextEmbedding(model_name=DENSE_MODEL)
        return self._dense_embedder

    @property
    def sparse_embedder(self) -> Any:
        if self._sparse_embedder is None:
            self._sparse_embedder = SparseTextEmbedding(model_name=SPARSE_MODEL)
        return self._sparse_embedder

    def ensure_collection(self) -> None:
        if not self.client.collection_exists(COLLECTION_NAME):
            self.client.create_collection(
                collection_name=COLLECTION_NAME,
                vectors_config={
                    "dense": models.VectorParams(size=DENSE_SIZE, distance=models.Distance.COSINE),
                },
                sparse_vectors_config={
                    "sparse": models.SparseVectorParams(modifier=models.Modifier.IDF),
                },
            )
            for field_name in PAYLOAD_INDEX_FIELDS:
                self.client.create_payload_index(
                    collection_name=COLLECTION_NAME,
                    field_name=field_name,
                    field_schema=models.PayloadSchemaType.KEYWORD,
                )
            return

        info = self.client.get_collection(COLLECTION_NAME)
        params = info.config.params
        vectors = params.vectors if isinstance(params.vectors, dict) else {}
        sparse_vectors = params.sparse_vectors or {}
        dense = vectors.get("dense")
        compatible = (
            dense is not None
            and int(getattr(dense, "size", -1)) == DENSE_SIZE
            and _enum_value(getattr(dense, "distance", "")) == "cosine"
            and "sparse" in sparse_vectors
        )
        if not compatible:
            raise RuntimeError(
                f"existing {COLLECTION_NAME!r} collection is incompatible with "
                f"dense={DENSE_MODEL}/{DENSE_SIZE}/cosine and sparse={SPARSE_MODEL}"
            )
        payload_schema = getattr(info, "payload_schema", {}) or {}
        for field_name in PAYLOAD_INDEX_FIELDS:
            if field_name in payload_schema:
                continue
            self.client.create_payload_index(
                collection_name=COLLECTION_NAME,
                field_name=field_name,
                field_schema=models.PayloadSchemaType.KEYWORD,
            )

    @staticmethod
    def _sparse_vector(embedding: Any) -> models.SparseVector:
        return models.SparseVector(
            indices=[int(value) for value in embedding.indices.tolist()],
            values=[float(value) for value in embedding.values.tolist()],
        )

    def upsert(
        self,
        documents: Sequence[IndexDocument],
        *,
        batch_size: int = 64,
        include_dense: bool = True,
        max_payload_bytes: int = DEFAULT_MAX_BATCH_PAYLOAD_BYTES,
    ) -> int:
        if not documents:
            return 0
        if batch_size < 1:
            raise ValueError("batch size must be positive")
        if max_payload_bytes < 1:
            raise ValueError("max payload bytes must be positive")
        self.ensure_collection()
        inserted = 0
        batches: list[list[IndexDocument]] = []
        current: list[IndexDocument] = []
        current_bytes = 0
        for document in documents:
            payload_bytes = len(
                json.dumps(document.payload, ensure_ascii=False, separators=(",", ":"), default=str).encode("utf-8")
            )
            if current and (len(current) >= batch_size or current_bytes + payload_bytes > max_payload_bytes):
                batches.append(current)
                current = []
                current_bytes = 0
            current.append(document)
            current_bytes += payload_bytes
        if current:
            batches.append(current)

        for batch in batches:
            texts = [document.text for document in batch]
            dense_texts = [text[:DENSE_TEXT_MAX_CHARS] for text in texts]
            dense_vectors = list(self.dense_embedder.embed(dense_texts)) if include_dense else [None] * len(batch)
            sparse_vectors = list(self.sparse_embedder.embed(texts))
            if len(dense_vectors) != len(batch) or len(sparse_vectors) != len(batch):
                raise RuntimeError("embedding count did not match document count")
            points = []
            for document, dense, sparse in zip(batch, dense_vectors, sparse_vectors, strict=True):
                vectors: dict[str, Any] = {
                    "sparse": self._sparse_vector(sparse),
                }
                if dense is not None:
                    vectors["dense"] = [float(value) for value in dense.tolist()]
                points.append(
                    models.PointStruct(
                        id=point_id(document.url),
                        vector=vectors,
                        payload=document.payload,
                    )
                )
            self.client.upsert(
                collection_name=COLLECTION_NAME,
                points=points,
                wait=True,
            )
            inserted += len(points)
        return inserted

    def _query_dense(self, query: str) -> list[float]:
        method = getattr(self.dense_embedder, "query_embed", self.dense_embedder.embed)
        vector = next(iter(method([query])))
        return [float(value) for value in vector.tolist()]

    def _query_sparse(self, query: str) -> models.SparseVector:
        method = getattr(self.sparse_embedder, "query_embed", self.sparse_embedder.embed)
        embedding = next(iter(method([query])))
        return self._sparse_vector(embedding)

    def search(
        self,
        query: str,
        *,
        limit: int = 10,
        filters: models.Filter | None = None,
    ) -> list[dict[str, Any]]:
        if not query.strip():
            return []
        candidate_limit = max(limit * 4, 20)
        response = self.client.query_points(
            collection_name=COLLECTION_NAME,
            prefetch=[
                models.Prefetch(query=self._query_dense(query), using="dense", limit=candidate_limit),
                models.Prefetch(query=self._query_sparse(query), using="sparse", limit=candidate_limit),
            ],
            query=models.FusionQuery(fusion=models.Fusion.RRF),
            query_filter=filters,
            limit=limit,
            with_payload=True,
            with_vectors=False,
        )
        return [
            {
                "id": str(point.id),
                "score": float(point.score),
                "payload": point.payload or {},
            }
            for point in response.points
        ]

from __future__ import annotations

from collections.abc import Iterable, Sequence
from typing import Any

from fastembed import SparseTextEmbedding, TextEmbedding
from qdrant_client import QdrantClient, models

from .corpus import IndexDocument, point_id


COLLECTION_NAME = "linkedin-full"
DENSE_MODEL = "BAAI/bge-small-en-v1.5"
SPARSE_MODEL = "Qdrant/bm25"
DENSE_SIZE = 384


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
            for field_name in (
                "url",
                "entity_type",
                "name",
                "company",
                "email",
                "location",
                "industry",
                "source_files",
            ):
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

    @staticmethod
    def _sparse_vector(embedding: Any) -> models.SparseVector:
        return models.SparseVector(
            indices=[int(value) for value in embedding.indices.tolist()],
            values=[float(value) for value in embedding.values.tolist()],
        )

    def upsert(self, documents: Sequence[IndexDocument], *, batch_size: int = 64) -> int:
        if not documents:
            return 0
        self.ensure_collection()
        inserted = 0
        for start in range(0, len(documents), batch_size):
            batch = documents[start : start + batch_size]
            texts = [document.text for document in batch]
            dense_vectors = list(self.dense_embedder.embed(texts))
            sparse_vectors = list(self.sparse_embedder.embed(texts))
            if len(dense_vectors) != len(batch) or len(sparse_vectors) != len(batch):
                raise RuntimeError("embedding count did not match document count")
            points = [
                models.PointStruct(
                    id=point_id(document.url),
                    vector={
                        "dense": [float(value) for value in dense.tolist()],
                        "sparse": self._sparse_vector(sparse),
                    },
                    payload=document.payload,
                )
                for document, dense, sparse in zip(batch, dense_vectors, sparse_vectors, strict=True)
            ]
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


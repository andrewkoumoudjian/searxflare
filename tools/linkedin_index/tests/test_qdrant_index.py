from __future__ import annotations

from types import SimpleNamespace

import numpy as np
import pytest

from linkedin_index.corpus import IndexDocument
from linkedin_index.qdrant_index import (
    COLLECTION_NAME,
    DENSE_MODEL,
    SPARSE_MODEL,
    LinkedInIndex,
)


class DenseEmbedder:
    def embed(self, texts):
        for _ in texts:
            yield np.ones(384, dtype=np.float32)


class SparseEmbedder:
    def embed(self, texts):
        for _ in texts:
            yield SimpleNamespace(
                indices=np.asarray([1, 7, 11], dtype=np.int32),
                values=np.asarray([0.5, 1.0, 0.2], dtype=np.float32),
            )

    def query_embed(self, texts):
        yield from self.embed(texts)


class FakeClient:
    def __init__(self, exists=False, dense_size=384, has_sparse=True):
        self.exists = exists
        self.dense_size = dense_size
        self.has_sparse = has_sparse
        self.created = None
        self.upserted = None
        self.query_request = None

    def collection_exists(self, collection_name):
        assert collection_name == COLLECTION_NAME
        return self.exists

    def create_collection(self, **kwargs):
        self.created = kwargs
        self.exists = True

    def create_payload_index(self, **kwargs):
        return None

    def get_collection(self, collection_name):
        vectors = {"dense": SimpleNamespace(size=self.dense_size, distance="Cosine")}
        sparse = {"sparse": SimpleNamespace()} if self.has_sparse else {}
        return SimpleNamespace(
            config=SimpleNamespace(
                params=SimpleNamespace(vectors=vectors, sparse_vectors=sparse)
            )
        )

    def upsert(self, **kwargs):
        self.upserted = kwargs

    def query_points(self, **kwargs):
        self.query_request = kwargs
        return SimpleNamespace(points=[SimpleNamespace(id="x", score=1.0, payload={"name": "Example"})])


def test_model_and_collection_contract_is_pinned():
    assert COLLECTION_NAME == "linkedin-full"
    assert DENSE_MODEL == "BAAI/bge-small-en-v1.5"
    assert SPARSE_MODEL == "Qdrant/bm25"


def test_ensure_collection_creates_named_dense_and_sparse_vectors():
    client = FakeClient(exists=False)
    index = LinkedInIndex(client=client, dense_embedder=DenseEmbedder(), sparse_embedder=SparseEmbedder())
    index.ensure_collection()

    assert set(client.created["vectors_config"]) == {"dense"}
    assert client.created["vectors_config"]["dense"].size == 384
    assert set(client.created["sparse_vectors_config"]) == {"sparse"}


def test_ensure_collection_rejects_incompatible_existing_vectors():
    client = FakeClient(exists=True, dense_size=768)
    index = LinkedInIndex(client=client, dense_embedder=DenseEmbedder(), sparse_embedder=SparseEmbedder())
    with pytest.raises(RuntimeError, match="incompatible"):
        index.ensure_collection()


def test_upsert_uses_deterministic_named_dense_and_sparse_vectors():
    client = FakeClient(exists=False)
    index = LinkedInIndex(client=client, dense_embedder=DenseEmbedder(), sparse_embedder=SparseEmbedder())
    document = IndexDocument(
        url="https://www.linkedin.com/in/example",
        text="food safety operations",
        payload={"url": "https://www.linkedin.com/in/example", "name": "Example"},
    )
    index.upsert([document])

    point = client.upserted["points"][0]
    assert set(point.vector) == {"dense", "sparse"}
    assert len(point.vector["dense"]) == 384
    assert point.vector["sparse"].indices == [1, 7, 11]


def test_search_uses_dense_sparse_prefetch_and_rrf():
    client = FakeClient(exists=True)
    index = LinkedInIndex(client=client, dense_embedder=DenseEmbedder(), sparse_embedder=SparseEmbedder())
    results = index.search("Montreal Manulife wealth advisor", limit=5)

    request = client.query_request
    assert request["collection_name"] == COLLECTION_NAME
    assert [prefetch.using for prefetch in request["prefetch"]] == ["dense", "sparse"]
    assert request["query"].fusion.value == "rrf"
    assert request["limit"] == 5
    assert results[0]["payload"]["name"] == "Example"


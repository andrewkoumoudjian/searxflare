from __future__ import annotations

import os
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
    def __init__(self):
        self.calls = 0
        self.last_texts = []

    def embed(self, texts):
        self.calls += 1
        self.last_texts = list(texts)
        for _ in self.last_texts:
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
        self.upsert_calls = []
        self.query_request = None
        self.payload_indexes = []

    def collection_exists(self, collection_name):
        assert collection_name == COLLECTION_NAME
        return self.exists

    def create_collection(self, **kwargs):
        self.created = kwargs
        self.exists = True

    def create_payload_index(self, **kwargs):
        self.payload_indexes.append(kwargs["field_name"])
        return None

    def get_collection(self, collection_name):
        vectors = {"dense": SimpleNamespace(size=self.dense_size, distance="Cosine")}
        sparse = {"sparse": SimpleNamespace()} if self.has_sparse else {}
        return SimpleNamespace(
            config=SimpleNamespace(
                params=SimpleNamespace(vectors=vectors, sparse_vectors=sparse)
            ),
            payload_schema={},
        )

    def upsert(self, **kwargs):
        self.upserted = kwargs
        self.upsert_calls.append(kwargs)

    def query_points(self, **kwargs):
        self.query_request = kwargs
        return SimpleNamespace(points=[SimpleNamespace(id="x", score=1.0, payload={"name": "Example"})])


def test_model_and_collection_contract_is_pinned():
    assert COLLECTION_NAME == "linkedin-full"
    assert DENSE_MODEL == "BAAI/bge-small-en-v1.5"
    assert SPARSE_MODEL == "Qdrant/bm25"
    assert os.environ.get("HF_HUB_DISABLE_IMPLICIT_TOKEN") == "1"


def test_ensure_collection_creates_named_dense_and_sparse_vectors():
    client = FakeClient(exists=False)
    index = LinkedInIndex(client=client, dense_embedder=DenseEmbedder(), sparse_embedder=SparseEmbedder())
    index.ensure_collection()

    assert set(client.created["vectors_config"]) == {"dense"}
    assert client.created["vectors_config"]["dense"].size == 384
    assert set(client.created["sparse_vectors_config"]) == {"sparse"}
    assert "aliases" in client.payload_indexes
    assert "title" in client.payload_indexes


def test_ensure_collection_adds_missing_payload_indexes_to_existing_collection():
    client = FakeClient(exists=True)
    index = LinkedInIndex(client=client, dense_embedder=DenseEmbedder(), sparse_embedder=SparseEmbedder())

    index.ensure_collection()

    assert "aliases" in client.payload_indexes
    assert "content_sources" in client.payload_indexes


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


def test_upsert_splits_batches_by_payload_bytes_before_qdrant_request():
    client = FakeClient(exists=False)
    index = LinkedInIndex(client=client, dense_embedder=DenseEmbedder(), sparse_embedder=SparseEmbedder())
    documents = [
        IndexDocument(
            url=f"https://www.linkedin.com/in/example-{number}",
            text="example",
            payload={"url": f"https://www.linkedin.com/in/example-{number}", "html": "x" * 12_000},
        )
        for number in range(3)
    ]

    inserted = index.upsert(documents, batch_size=64, max_payload_bytes=20_000)

    assert inserted == 3
    assert len(client.upsert_calls) == 3


def test_sparse_only_upsert_skips_dense_embedding_and_omits_dense_vector():
    client = FakeClient(exists=False)
    dense = DenseEmbedder()
    index = LinkedInIndex(client=client, dense_embedder=dense, sparse_embedder=SparseEmbedder())
    document = IndexDocument(
        url="https://www.linkedin.com/in/sparse-only",
        text="chief financial officer Montreal",
        payload={"url": "https://www.linkedin.com/in/sparse-only", "name": "Sparse Only"},
    )

    index.upsert([document], include_dense=False)

    point = client.upserted["points"][0]
    assert set(point.vector) == {"sparse"}
    assert dense.calls == 0


def test_dense_upsert_caps_dense_projection_without_truncating_sparse_input():
    client = FakeClient(exists=False)
    dense = DenseEmbedder()
    sparse = SparseEmbedder()
    index = LinkedInIndex(client=client, dense_embedder=dense, sparse_embedder=sparse)
    long_text = "important profile text " * 200
    document = IndexDocument(
        url="https://www.linkedin.com/in/long",
        text=long_text,
        payload={"url": "https://www.linkedin.com/in/long"},
    )

    index.upsert([document], include_dense=True)

    assert len(dense.last_texts[0]) == 1000
    assert document.text == long_text


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

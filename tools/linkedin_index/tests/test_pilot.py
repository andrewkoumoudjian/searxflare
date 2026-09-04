from __future__ import annotations

from linkedin_index.pilot import PilotManifest, chunk_digest, chunk_seeds


def test_chunk_seeds_canonicalizes_deduplicates_and_is_stable():
    chunks = chunk_seeds(
        [
            "https://linkedin.com/in/a/?trk=x",
            "https://www.linkedin.com/in/b",
            "https://www.linkedin.com/in/a",
            "https://example.com/not-linkedin",
            "https://ca.linkedin.com/company/acme/",
        ],
        size=2,
    )

    assert chunks == [
        ["https://www.linkedin.com/in/a", "https://www.linkedin.com/in/b"],
        ["https://www.linkedin.com/company/acme"],
    ]
    assert chunk_digest(chunks[0]) == chunk_digest(list(chunks[0]))


def test_manifest_skips_only_matching_completed_chunks(tmp_path):
    chunks = chunk_seeds(
        [
            "https://www.linkedin.com/in/a",
            "https://www.linkedin.com/in/b",
            "https://www.linkedin.com/in/c",
        ],
        size=2,
    )
    manifest_path = tmp_path / "manifest.json"
    manifest = PilotManifest.create(manifest_path, chunks)
    manifest.mark_completed(
        0,
        run_id="run-1",
        dataset_id="dataset-1",
        dataset_path=str(tmp_path / "chunk-000.json"),
    )
    manifest.save()

    resumed = PilotManifest.load(manifest_path, chunks)
    assert [chunk.index for chunk in resumed.pending_chunks()] == [1]


def test_manifest_requeues_failed_and_changed_chunks(tmp_path):
    original = [["https://www.linkedin.com/in/a"]]
    manifest_path = tmp_path / "manifest.json"
    manifest = PilotManifest.create(manifest_path, original)
    manifest.mark_failed(0, run_id="run-bad", error="timed out")
    manifest.save()

    resumed_failed = PilotManifest.load(manifest_path, original)
    assert [chunk.index for chunk in resumed_failed.pending_chunks()] == [0]

    changed = [["https://www.linkedin.com/in/b"]]
    resumed_changed = PilotManifest.load(manifest_path, changed)
    assert [chunk.index for chunk in resumed_changed.pending_chunks()] == [0]
    assert resumed_changed.chunks[0].digest == chunk_digest(changed[0])


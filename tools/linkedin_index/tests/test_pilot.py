from __future__ import annotations

import json
import subprocess

from linkedin_index import pilot as pilot_module
from linkedin_index.pilot import (
    PilotManifest,
    chunk_digest,
    chunk_seeds,
    extract_run_id,
    recovery_seeds_from_manifest,
)


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


def test_recovery_seeds_from_manifest_excludes_aliases_that_succeeded(tmp_path):
    primary = tmp_path / "primary"
    primary.mkdir()
    (primary / "chunk-000.json").write_text(
        """[
          {"url":"https://www.linkedin.com/in/s%c3%a9rgio", "status":999, "fetched":false},
          {"url":"https://www.linkedin.com/in/sérgio", "status":200, "fetched":true},
          {"url":"https://www.linkedin.com/in/blocked", "status":999, "fetched":false},
          {"url":"https://www.linkedin.com/in/gone", "status":404, "fetched":false}
        ]""",
        encoding="utf-8",
    )
    manifest = {
        "chunks": [
            {
                "index": 0,
                "digest": "x",
                "seeds": [],
                "status": "SUCCEEDED",
                "dataset_path": str(primary / "chunk-000.json"),
            }
        ]
    }
    manifest_path = primary / "manifest.json"
    manifest_path.write_text(__import__("json").dumps(manifest), encoding="utf-8")

    assert recovery_seeds_from_manifest(manifest_path) == [
        "https://www.linkedin.com/in/blocked"
    ]


def test_extract_run_id_prefers_run_object_over_actor_id():
    payload = {
        "actor": {"id": "TmxkNceI631zStdDV"},
        "run": {"id": "actual-run-id", "status": "READY"},
    }
    assert extract_run_id(payload) == "actual-run-id"


def test_run_json_retries_once_when_cli_fails_without_a_run(monkeypatch):
    results = iter(
        [
            subprocess.CompletedProcess(["apify"], 1, stdout="", stderr="temporary start failure"),
            subprocess.CompletedProcess(
                ["apify"],
                0,
                stdout=json.dumps({"run": {"id": "run-after-retry"}}),
                stderr="",
            ),
        ]
    )
    calls = []

    def fake_run(*args, **kwargs):
        calls.append((args, kwargs))
        return next(results)

    monkeypatch.setattr(pilot_module.subprocess, "run", fake_run)
    monkeypatch.setattr(pilot_module.time, "sleep", lambda _seconds: None)

    payload = pilot_module._run_json(["apify", "actors", "start"], retries=1)

    assert payload["run"]["id"] == "run-after-retry"
    assert len(calls) == 2

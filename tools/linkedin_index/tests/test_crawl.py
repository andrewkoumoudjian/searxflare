import json
from pathlib import Path

import linkedin_index.crawl as crawl_mod
from linkedin_index.crawl import (
    CrawlState,
    bootstrap_depth_from_datasets,
    discover_next_frontier,
    select_frontier,
)


def test_discover_next_frontier_uses_only_successful_canonical_links():
    rows = [
        {
            "url": "https://www.linkedin.com/in/seed-a",
            "status": 200,
            "fetched": True,
            "links": [
                "https://ca.linkedin.com/company/acme/?trk=profile",
                "https://www.linkedin.com/in/next-person/",
                "https://example.com/not-linkedin",
            ],
        },
        {
            "url": "https://www.linkedin.com/in/seed-b",
            "status": 999,
            "fetched": False,
            "links": ["https://www.linkedin.com/in/should-not-expand"],
        },
    ]

    assert discover_next_frontier(rows, known={"https://www.linkedin.com/in/seed-a"}) == [
        "https://www.linkedin.com/company/acme",
        "https://www.linkedin.com/in/next-person",
    ]


def test_crawl_state_absorbs_depth_and_persists_frontier(tmp_path: Path):
    state = CrawlState.create(
        tmp_path / "crawl-state.json",
        ["https://linkedin.com/in/seed-a/", "https://www.linkedin.com/in/seed-b"],
        max_depth=2,
    )
    rows = [
        {
            "url": "https://www.linkedin.com/in/seed-a",
            "status": 200,
            "fetched": True,
            "links": [
                "https://www.linkedin.com/company/acme",
                "https://www.linkedin.com/in/seed-b",
            ],
        },
        {
            "url": "https://www.linkedin.com/in/seed-b",
            "status": 999,
            "fetched": False,
            "links": [],
        },
    ]

    state.absorb_depth(0, rows)
    state.save()
    restored = CrawlState.load(
        state.path,
        ["https://www.linkedin.com/in/seed-a", "https://www.linkedin.com/in/seed-b"],
        max_depth=2,
    )

    assert restored.processed_depths == {0}
    assert restored.successful == {"https://www.linkedin.com/in/seed-a"}
    assert restored.failures == {"https://www.linkedin.com/in/seed-b": 999}
    assert restored.frontier[1] == ["https://www.linkedin.com/company/acme"]


def test_crawl_state_records_next_frontier_even_at_current_depth_limit(tmp_path: Path):
    state = CrawlState.create(
        tmp_path / "crawl-state.json",
        ["https://www.linkedin.com/in/seed-a"],
        max_depth=0,
    )
    state.absorb_depth(
        0,
        [
            {
                "url": "https://www.linkedin.com/in/seed-a",
                "status": 200,
                "fetched": True,
                "links": ["https://www.linkedin.com/company/acme"],
            }
        ],
    )

    assert state.frontier[1] == ["https://www.linkedin.com/company/acme"]


def test_crawl_state_rejects_a_different_seed_universe(tmp_path: Path):
    state = CrawlState.create(
        tmp_path / "crawl-state.json",
        ["https://www.linkedin.com/in/seed-a"],
        max_depth=1,
    )
    state.save()

    try:
        CrawlState.load(
            state.path,
            ["https://www.linkedin.com/in/different"],
            max_depth=1,
        )
    except ValueError as exc:
        assert "seed universe" in str(exc)
    else:
        raise AssertionError("expected seed mismatch to be rejected")


def test_select_frontier_prioritizes_companies_then_people_and_defers_leaf_types():
    urls = [
        "https://www.linkedin.com/posts/example-activity-1",
        "https://www.linkedin.com/in/person-b",
        "https://www.linkedin.com/company/company-a",
        "https://www.linkedin.com/in/person-a",
        "https://www.linkedin.com/company/company-b",
        "https://www.linkedin.com/school/example-school",
    ]

    assert select_frontier(
        urls,
        visited={"https://www.linkedin.com/company/company-b"},
        include_roots={"company", "in"},
        root_priority=("company", "in"),
        limit=2,
    ) == [
        "https://www.linkedin.com/company/company-a",
        "https://www.linkedin.com/in/person-b",
    ]


def test_partial_depth_absorb_merges_discovered_frontier_without_marking_depth_complete(tmp_path: Path):
    state = CrawlState.create(
        tmp_path / "crawl-state.json",
        ["https://www.linkedin.com/in/seed"],
        max_depth=2,
    )
    state.frontier[1] = [
        "https://www.linkedin.com/company/a",
        "https://www.linkedin.com/company/b",
    ]

    state.absorb_depth(
        1,
        [
            {
                "url": "https://www.linkedin.com/company/a",
                "status": 200,
                "fetched": True,
                "links": ["https://www.linkedin.com/in/person-a"],
            }
        ],
        complete=False,
    )
    state.absorb_depth(
        1,
        [
            {
                "url": "https://www.linkedin.com/company/b",
                "status": 200,
                "fetched": True,
                "links": ["https://www.linkedin.com/in/person-b"],
            }
        ],
        complete=True,
    )

    assert state.frontier[2] == [
        "https://www.linkedin.com/in/person-a",
        "https://www.linkedin.com/in/person-b",
    ]
    assert 1 in state.processed_depths


def test_crawl_state_round_trips_immutable_depth_waves(tmp_path: Path):
    state = CrawlState.create(
        tmp_path / "crawl-state.json",
        ["https://www.linkedin.com/in/seed"],
        max_depth=1,
    )
    state.depth_waves = {
        0: [str(tmp_path / "depth-000" / "wave-000" / "manifest.json")],
        1: [
            str(tmp_path / "depth-001" / "wave-000" / "manifest.json"),
            str(tmp_path / "depth-001" / "wave-001" / "manifest.json"),
        ],
    }
    state.save()

    restored = CrawlState.load(
        state.path,
        ["https://www.linkedin.com/in/seed"],
        max_depth=1,
    )

    assert restored.depth_waves == state.depth_waves


def test_crawl_state_migrates_legacy_single_depth_manifest_to_wave_list(tmp_path: Path):
    state = CrawlState.create(
        tmp_path / "crawl-state.json",
        ["https://www.linkedin.com/in/seed"],
        max_depth=1,
    )
    state.save()
    payload = __import__("json").loads(state.path.read_text(encoding="utf-8"))
    payload.pop("depth_waves", None)
    payload["depth_manifests"] = {
        "0": str(tmp_path / "depth-000" / "manifest.json")
    }
    state.path.write_text(__import__("json").dumps(payload), encoding="utf-8")

    restored = CrawlState.load(
        state.path,
        ["https://www.linkedin.com/in/seed"],
        max_depth=1,
    )

    assert restored.depth_waves == {
        0: [str(tmp_path / "depth-000" / "manifest.json")]
    }


def test_execute_crawl_does_not_advance_depth_until_current_frontier_is_exhausted(
    tmp_path: Path,
    monkeypatch,
):
    seeds = [
        "https://www.linkedin.com/in/seed-a",
        "https://www.linkedin.com/in/seed-b",
    ]
    state = CrawlState.create(tmp_path / "crawl-state.json", seeds, max_depth=1)
    executed_depths: list[int] = []

    def fake_execute_manifest(manifest, out_dir, **_kwargs):
        depth = int(out_dir.parent.name.removeprefix("depth-"))
        executed_depths.append(depth)
        for chunk in manifest.chunks:
            dataset_path = out_dir / f"chunk-{chunk.index:03d}.json"
            dataset_path.parent.mkdir(parents=True, exist_ok=True)
            rows = [
                {
                    "url": url,
                    "status": 200,
                    "fetched": True,
                    "links": ["https://www.linkedin.com/in/child"],
                }
                for url in chunk.seeds
            ]
            dataset_path.write_text(json.dumps(rows), encoding="utf-8")
            manifest.mark_completed(
                chunk.index,
                run_id=f"run-{depth}-{chunk.index}",
                dataset_id=f"dataset-{depth}-{chunk.index}",
                dataset_path=str(dataset_path),
            )
        manifest.save()
        return manifest

    monkeypatch.setattr(crawl_mod, "execute_manifest", fake_execute_manifest)

    crawl_mod.execute_crawl(
        state,
        tmp_path / "crawl",
        chunk_size=1,
        max_nodes_per_depth=1,
    )

    assert executed_depths == [0]
    assert state.visited == {"https://www.linkedin.com/in/seed-a"}
    assert "https://www.linkedin.com/in/seed-b" in state.frontier[0]
    assert state.frontier[1] == ["https://www.linkedin.com/in/child"]
    assert 0 not in state.processed_depths


def test_bootstrap_depth_from_datasets_reuses_paid_rows_and_leaves_only_unseen_seeds(
    tmp_path: Path,
):
    seeds = [
        "https://www.linkedin.com/in/seed-a",
        "https://www.linkedin.com/in/seed-b",
    ]
    state = CrawlState.create(tmp_path / "crawl-state.json", seeds, max_depth=1)
    dataset = tmp_path / "paid-seed-run.json"
    dataset.write_text(
        json.dumps(
            [
                {
                    "url": "https://linkedin.com/in/seed-a/",
                    "status": 200,
                    "fetched": True,
                    "links": ["https://www.linkedin.com/company/acme"],
                }
            ]
        ),
        encoding="utf-8",
    )

    rows = bootstrap_depth_from_datasets(state, 0, [dataset])

    assert rows == 1
    assert state.visited == {"https://www.linkedin.com/in/seed-a"}
    assert state.successful == {"https://www.linkedin.com/in/seed-a"}
    assert state.frontier[1] == ["https://www.linkedin.com/company/acme"]
    assert select_frontier(state.frontier[0], visited=state.visited) == [
        "https://www.linkedin.com/in/seed-b"
    ]
    assert 0 not in state.processed_depths


def test_bootstrap_depth_marks_depth_complete_when_dataset_accounts_for_all_seeds(
    tmp_path: Path,
):
    seeds = [
        "https://www.linkedin.com/in/seed-a",
        "https://www.linkedin.com/in/seed-b",
    ]
    state = CrawlState.create(tmp_path / "crawl-state.json", seeds, max_depth=1)
    dataset = tmp_path / "paid-seed-run.json"
    dataset.write_text(
        json.dumps(
            [
                {"url": seeds[0], "status": 200, "fetched": True, "links": []},
                {"url": seeds[1], "status": 999, "fetched": False, "links": []},
            ]
        ),
        encoding="utf-8",
    )

    rows = bootstrap_depth_from_datasets(state, 0, [dataset])

    assert rows == 2
    assert state.visited == set(seeds)
    assert state.failures == {seeds[1]: 999}
    assert 0 in state.processed_depths


def test_crawl_cli_accepts_paid_depth_zero_dataset_without_recrawling(tmp_path: Path):
    seeds = [
        "https://www.linkedin.com/in/seed-a",
        "https://www.linkedin.com/in/seed-b",
    ]
    seed_path = tmp_path / "seeds.json"
    seed_path.write_text(json.dumps(seeds), encoding="utf-8")
    dataset = tmp_path / "paid.json"
    dataset.write_text(
        json.dumps(
            [
                {"url": seeds[0], "status": 200, "fetched": True, "links": []},
                {"url": seeds[1], "status": 999, "fetched": False, "links": []},
            ]
        ),
        encoding="utf-8",
    )
    out = tmp_path / "crawl"

    assert crawl_mod.main(
        [
            "--seeds",
            str(seed_path),
            "--out",
            str(out),
            "--max-depth",
            "0",
            "--bootstrap-dataset",
            str(dataset),
        ]
    ) == 0

    state = CrawlState.load(out / "crawl-state.json", seeds, max_depth=0)
    assert state.visited == set(seeds)
    assert state.successful == {seeds[0]}
    assert state.failures == {seeds[1]: 999}
    assert state.depth_waves == {}

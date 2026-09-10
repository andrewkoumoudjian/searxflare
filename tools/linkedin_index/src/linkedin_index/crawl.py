from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Sequence

from .canonical import canonical_linkedin_url
from .pilot import DEFAULT_ACTOR_ID, PilotManifest, chunk_seeds, execute_manifest


def _canonical_unique(urls: Iterable[str]) -> list[str]:
    out: list[str] = []
    seen: set[str] = set()
    for raw in urls:
        url = canonical_linkedin_url(str(raw))
        if not url or url in seen:
            continue
        seen.add(url)
        out.append(url)
    return out


def _seed_digest(seeds: Sequence[str]) -> str:
    return hashlib.sha256("\n".join(seeds).encode("utf-8")).hexdigest()


def _url_root(url: str) -> str:
    return url.split("linkedin.com/", 1)[-1].split("/", 1)[0]


def select_frontier(
    urls: Sequence[str],
    *,
    visited: set[str],
    include_roots: set[str] | None = None,
    root_priority: Sequence[str] = ("company", "in", "showcase", "posts", "pulse", "school"),
    limit: int | None = None,
) -> list[str]:
    """Select uncrawled frontier nodes in a deterministic, yield-aware order."""
    canonical = _canonical_unique(urls)
    allowed = include_roots
    priority = {root: index for index, root in enumerate(root_priority)}
    position = {url: index for index, url in enumerate(canonical)}
    pending = [
        url
        for url in canonical
        if url not in visited and (allowed is None or _url_root(url) in allowed)
    ]
    ordered = sorted(
        pending,
        key=lambda url: (priority.get(_url_root(url), len(priority)), position[url]),
    )
    if limit is not None:
        if limit < 1:
            raise ValueError("frontier limit must be positive")
        return ordered[:limit]
    return ordered


def _is_success(row: dict[str, Any]) -> bool:
    try:
        return int(row.get("status")) == 200 and row.get("fetched") is not False
    except (TypeError, ValueError):
        return False


def discover_next_frontier(rows: Sequence[dict[str, Any]], *, known: set[str]) -> list[str]:
    frontier: list[str] = []
    seen = set(known)
    for row in rows:
        if not _is_success(row):
            continue
        links = row.get("links") or []
        if not isinstance(links, list):
            continue
        for raw in links:
            url = canonical_linkedin_url(str(raw))
            if not url or url in seen:
                continue
            seen.add(url)
            frontier.append(url)
    return frontier


def _load_dataset(path: Path) -> list[dict[str, Any]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(payload, dict):
        return [payload]
    if isinstance(payload, list) and all(isinstance(item, dict) for item in payload):
        return [dict(item) for item in payload]
    raise ValueError(f"dataset {path} must contain a JSON object or array of objects")


def rows_from_manifest(manifest: PilotManifest) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for chunk in manifest.chunks:
        if chunk.status != "SUCCEEDED" or not chunk.dataset_path:
            continue
        rows.extend(_load_dataset(Path(chunk.dataset_path)))
    return rows


def bootstrap_depth_from_datasets(
    state: "CrawlState",
    depth: int,
    paths: Sequence[str | Path],
) -> int:
    """Absorb already-paid crawl datasets into state without issuing new requests."""
    rows: list[dict[str, Any]] = []
    for raw_path in paths:
        rows.extend(_load_dataset(Path(raw_path)))
    if not rows:
        return 0

    state.absorb_depth(depth, rows, complete=False)
    remaining = select_frontier(state.frontier.get(depth, []), visited=state.visited)
    if remaining:
        state.processed_depths.discard(depth)
    else:
        state.processed_depths.add(depth)
    return len(rows)


@dataclass
class CrawlState:
    path: Path
    seed_digest: str
    max_depth: int
    frontier: dict[int, list[str]] = field(default_factory=dict)
    visited: set[str] = field(default_factory=set)
    successful: set[str] = field(default_factory=set)
    failures: dict[str, int | None] = field(default_factory=dict)
    processed_depths: set[int] = field(default_factory=set)
    depth_waves: dict[int, list[str]] = field(default_factory=dict)
    version: int = 1

    @classmethod
    def create(
        cls,
        path: str | Path,
        seeds: Sequence[str],
        *,
        max_depth: int,
    ) -> "CrawlState":
        if max_depth < 0:
            raise ValueError("max depth must be non-negative")
        canonical = _canonical_unique(seeds)
        return cls(
            path=Path(path),
            seed_digest=_seed_digest(canonical),
            max_depth=max_depth,
            frontier={0: canonical},
        )

    @classmethod
    def load(
        cls,
        path: str | Path,
        seeds: Sequence[str],
        *,
        max_depth: int,
    ) -> "CrawlState":
        state_path = Path(path)
        canonical = _canonical_unique(seeds)
        digest = _seed_digest(canonical)
        if not state_path.exists():
            return cls.create(state_path, canonical, max_depth=max_depth)
        raw = json.loads(state_path.read_text(encoding="utf-8"))
        if raw.get("seed_digest") != digest:
            raise ValueError("crawl state seed universe does not match requested seeds")
        raw_waves = raw.get("depth_waves")
        if isinstance(raw_waves, dict):
            depth_waves = {
                int(k): ([str(item) for item in v] if isinstance(v, list) else [str(v)])
                for k, v in raw_waves.items()
            }
        else:
            legacy_manifests = raw.get("depth_manifests") or {}
            depth_waves = {
                int(k): [str(v)]
                for k, v in legacy_manifests.items()
                if isinstance(v, str) and v
            }

        state = cls(
            path=state_path,
            seed_digest=digest,
            max_depth=max(int(raw.get("max_depth", 0)), max_depth),
            frontier={int(k): list(v) for k, v in (raw.get("frontier") or {}).items()},
            visited=set(raw.get("visited") or []),
            successful=set(raw.get("successful") or []),
            failures={str(k): v for k, v in (raw.get("failures") or {}).items()},
            processed_depths={int(v) for v in (raw.get("processed_depths") or [])},
            depth_waves=depth_waves,
            version=int(raw.get("version", 1)),
        )
        state.frontier.setdefault(0, canonical)
        return state

    def known_urls(self) -> set[str]:
        known = set(self.visited)
        for urls in self.frontier.values():
            known.update(urls)
        return known

    def absorb_depth(
        self,
        depth: int,
        rows: Sequence[dict[str, Any]],
        *,
        complete: bool = True,
    ) -> None:
        attempted = _canonical_unique(str(row.get("url") or "") for row in rows)
        self.visited.update(attempted)
        for row in rows:
            url = canonical_linkedin_url(str(row.get("url") or ""))
            if not url:
                continue
            if _is_success(row):
                self.successful.add(url)
                self.failures.pop(url, None)
                continue
            try:
                status = int(row.get("status")) if row.get("status") is not None else None
            except (TypeError, ValueError):
                status = None
            self.failures[url] = status

        next_urls = discover_next_frontier(rows, known=self.known_urls())
        if next_urls:
            existing = self.frontier.get(depth + 1, [])
            self.frontier[depth + 1] = _canonical_unique([*existing, *next_urls])
        if complete:
            self.processed_depths.add(depth)

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "version": self.version,
            "seed_digest": self.seed_digest,
            "max_depth": self.max_depth,
            "frontier": {str(k): v for k, v in sorted(self.frontier.items())},
            "visited": sorted(self.visited),
            "successful": sorted(self.successful),
            "failures": dict(sorted(self.failures.items())),
            "processed_depths": sorted(self.processed_depths),
            "depth_waves": {str(k): v for k, v in sorted(self.depth_waves.items())},
        }
        temp = self.path.with_suffix(self.path.suffix + ".tmp")
        temp.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
        temp.replace(self.path)


def _load_seed_file(path: Path) -> list[str]:
    if path.suffix.lower() == ".json":
        payload = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(payload, list):
            raise ValueError("JSON seed file must contain an array")
        return [item["url"] if isinstance(item, dict) else str(item) for item in payload]
    return [line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def execute_crawl(
    state: CrawlState,
    out_dir: Path,
    *,
    chunk_size: int = 50,
    concurrency: int = 8,
    attempts: int = 3,
    timeout: int = 3600,
    actor_id: str = DEFAULT_ACTOR_ID,
    include_roots: set[str] | None = None,
    max_nodes_per_depth: int | None = None,
) -> CrawlState:
    def manifest_succeeded(path: Path) -> bool:
        if not path.exists():
            return False
        raw = json.loads(path.read_text(encoding="utf-8"))
        chunks = raw.get("chunks", []) if isinstance(raw, dict) else []
        return bool(chunks) and all(
            isinstance(chunk, dict) and chunk.get("status") == "SUCCEEDED"
            for chunk in chunks
        )

    for depth in range(state.max_depth + 1):
        seeds = select_frontier(
            state.frontier.get(depth, []),
            visited=state.visited,
            include_roots=include_roots,
            limit=max_nodes_per_depth,
        )
        if not seeds:
            continue
        depth_root = out_dir / f"depth-{depth:03d}"
        chunks = chunk_seeds(seeds, chunk_size)
        waves = state.depth_waves.setdefault(depth, [])
        if waves and not manifest_succeeded(Path(waves[-1])):
            manifest_path = Path(waves[-1])
            depth_dir = manifest_path.parent
        else:
            wave_index = len(waves)
            depth_dir = depth_root / f"wave-{wave_index:03d}"
            manifest_path = depth_dir / "manifest.json"
            waves.append(str(manifest_path))
        manifest = PilotManifest.load(manifest_path, chunks, actor_id=actor_id)
        manifest.save()
        state.save()
        execute_manifest(
            manifest,
            depth_dir,
            concurrency=concurrency,
            attempts=attempts,
            timeout=timeout,
        )
        remaining = select_frontier(
            state.frontier.get(depth, []),
            visited=state.visited.union(seeds),
            include_roots=include_roots,
        )
        state.absorb_depth(depth, rows_from_manifest(manifest), complete=not remaining)
        state.save()
        if remaining:
            break
    return state


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Run a durable depth-by-depth LinkedIn public-SSR BFS crawl")
    parser.add_argument("--seeds", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--max-depth", type=int, default=1)
    parser.add_argument("--chunk-size", type=int, default=50)
    parser.add_argument("--concurrency", type=int, default=8)
    parser.add_argument("--attempts", type=int, default=3)
    parser.add_argument("--timeout", type=int, default=3600)
    parser.add_argument("--actor-id", default=DEFAULT_ACTOR_ID)
    parser.add_argument(
        "--bootstrap-dataset",
        type=Path,
        action="append",
        default=[],
        help="Reuse an existing depth-zero crawl dataset before issuing any new requests; repeat for multiple datasets",
    )
    parser.add_argument(
        "--include-root",
        action="append",
        choices=["in", "company", "showcase", "posts", "pulse", "school"],
        help="Only execute these LinkedIn path roots at each depth; repeat for multiple roots",
    )
    parser.add_argument(
        "--max-nodes-per-depth",
        type=int,
        help="Bound each invocation to the first N eligible uncrawled nodes per depth",
    )
    args = parser.parse_args(argv)

    seeds = _load_seed_file(args.seeds)
    state_path = args.out / "crawl-state.json"
    state = CrawlState.load(state_path, seeds, max_depth=args.max_depth)
    bootstrapped_rows = 0
    if args.bootstrap_dataset:
        bootstrapped_rows = bootstrap_depth_from_datasets(state, 0, args.bootstrap_dataset)
    state.save()
    execute_crawl(
        state,
        args.out,
        chunk_size=args.chunk_size,
        concurrency=args.concurrency,
        attempts=args.attempts,
        timeout=args.timeout,
        actor_id=args.actor_id,
        include_roots=set(args.include_root) if args.include_root else None,
        max_nodes_per_depth=args.max_nodes_per_depth,
    )
    print(json.dumps({
        "state": str(state_path),
        "max_depth": state.max_depth,
        "visited": len(state.visited),
        "successful": len(state.successful),
        "failed": len(state.failures),
        "bootstrapped_rows": bootstrapped_rows,
        "frontier": {str(k): len(v) for k, v in sorted(state.frontier.items())},
        "processed_depths": sorted(state.processed_depths),
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

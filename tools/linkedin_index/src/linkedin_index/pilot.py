from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable, Sequence

from .canonical import canonical_linkedin_url


DEFAULT_ACTOR_ID = "TmxkNceI631zStdDV"


def chunk_seeds(urls: Iterable[str], size: int) -> list[list[str]]:
    if size < 1:
        raise ValueError("chunk size must be positive")
    unique: list[str] = []
    seen: set[str] = set()
    for raw in urls:
        url = canonical_linkedin_url(raw)
        if url and url not in seen:
            seen.add(url)
            unique.append(url)
    return [unique[index : index + size] for index in range(0, len(unique), size)]


def chunk_digest(seeds: Sequence[str]) -> str:
    payload = "\n".join(seeds).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


@dataclass
class PilotChunk:
    index: int
    digest: str
    seeds: list[str]
    status: str = "PENDING"
    run_id: str | None = None
    dataset_id: str | None = None
    dataset_path: str | None = None
    error: str | None = None


@dataclass
class PilotManifest:
    path: Path
    chunks: list[PilotChunk]
    actor_id: str = DEFAULT_ACTOR_ID
    version: int = 1

    @classmethod
    def create(
        cls,
        path: str | Path,
        chunks: Sequence[Sequence[str]],
        actor_id: str = DEFAULT_ACTOR_ID,
    ) -> "PilotManifest":
        states = [
            PilotChunk(index=index, digest=chunk_digest(chunk), seeds=list(chunk))
            for index, chunk in enumerate(chunks)
        ]
        return cls(path=Path(path), chunks=states, actor_id=actor_id)

    @classmethod
    def load(
        cls,
        path: str | Path,
        chunks: Sequence[Sequence[str]],
        actor_id: str = DEFAULT_ACTOR_ID,
    ) -> "PilotManifest":
        manifest_path = Path(path)
        if not manifest_path.exists():
            return cls.create(manifest_path, chunks, actor_id=actor_id)
        raw = json.loads(manifest_path.read_text(encoding="utf-8"))
        old_by_index = {int(item["index"]): item for item in raw.get("chunks", [])}
        states: list[PilotChunk] = []
        for index, seeds in enumerate(chunks):
            seeds_list = list(seeds)
            digest = chunk_digest(seeds_list)
            old = old_by_index.get(index)
            if old and old.get("digest") == digest:
                states.append(PilotChunk(**{**old, "index": index, "digest": digest, "seeds": seeds_list}))
            else:
                states.append(PilotChunk(index=index, digest=digest, seeds=seeds_list))
        return cls(
            path=manifest_path,
            chunks=states,
            actor_id=raw.get("actor_id", actor_id),
            version=int(raw.get("version", 1)),
        )

    def pending_chunks(self) -> list[PilotChunk]:
        return [chunk for chunk in self.chunks if chunk.status != "SUCCEEDED"]

    def mark_running(self, index: int, run_id: str) -> None:
        chunk = self.chunks[index]
        chunk.status = "RUNNING"
        chunk.run_id = run_id
        chunk.error = None

    def mark_completed(
        self,
        index: int,
        *,
        run_id: str,
        dataset_id: str,
        dataset_path: str,
    ) -> None:
        chunk = self.chunks[index]
        chunk.status = "SUCCEEDED"
        chunk.run_id = run_id
        chunk.dataset_id = dataset_id
        chunk.dataset_path = dataset_path
        chunk.error = None

    def mark_failed(self, index: int, *, run_id: str | None, error: str) -> None:
        chunk = self.chunks[index]
        chunk.status = "FAILED"
        chunk.run_id = run_id
        chunk.error = error

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "version": self.version,
            "actor_id": self.actor_id,
            "chunks": [asdict(chunk) for chunk in self.chunks],
        }
        temp = self.path.with_suffix(self.path.suffix + ".tmp")
        temp.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
        temp.replace(self.path)


def _run_json(command: list[str], *, retries: int = 0) -> dict:
    attempt = 0
    while True:
        result = subprocess.run(command, check=False, text=True, capture_output=True)
        payload: dict | None = None
        if result.stdout.strip():
            try:
                decoded = json.loads(result.stdout)
            except json.JSONDecodeError:
                decoded = None
            if isinstance(decoded, dict):
                payload = decoded
        if result.returncode == 0:
            if payload is None:
                raise RuntimeError(
                    f"command succeeded without JSON output: {command!r}; stderr={result.stderr.strip()!r}"
                )
            return payload
        # Some CLI failures can happen after an Actor run was accepted. If a concrete
        # run id is present, adopt it instead of issuing a duplicate start request.
        run = payload.get("run") if isinstance(payload, dict) else None
        if isinstance(run, dict) and isinstance(run.get("id"), str):
            return payload
        if attempt >= retries:
            raise subprocess.CalledProcessError(
                result.returncode,
                command,
                output=result.stdout,
                stderr=result.stderr,
            )
        attempt += 1
        time.sleep(1.0)


def _find_key(value: object, key: str) -> object | None:
    if isinstance(value, dict):
        if key in value:
            return value[key]
        for child in value.values():
            found = _find_key(child, key)
            if found is not None:
                return found
    elif isinstance(value, list):
        for child in value:
            found = _find_key(child, key)
            if found is not None:
                return found
    return None


def extract_run_id(payload: dict) -> str:
    run = payload.get("run")
    if isinstance(run, dict) and isinstance(run.get("id"), str):
        return run["id"]
    run_id = payload.get("runId")
    if isinstance(run_id, str):
        return run_id
    raise RuntimeError(f"Apify start response did not contain a run id: {payload}")


def _actor_input(chunk: PilotChunk, concurrency: int, attempts: int) -> dict:
    return {
        "startUrls": [{"url": url} for url in chunk.seeds],
        "maxRequests": len(chunk.seeds),
        "maxDepth": 0,
        "maxLinksPerPage": 64,
        "useApifyProxy": True,
        "proxyGroups": ["auto"],
        "maxConcurrency": concurrency,
        "attemptsPerUrl": attempts,
        "interRequestDelay": 0.0,
    }


def _start_chunk(actor_id: str, chunk: PilotChunk, out_dir: Path, concurrency: int, attempts: int, timeout: int) -> str:
    out_dir.mkdir(parents=True, exist_ok=True)
    input_path = out_dir / f"chunk-{chunk.index:03d}-input.json"
    input_path.write_text(json.dumps(_actor_input(chunk, concurrency, attempts), indent=2), encoding="utf-8")
    payload = _run_json(
        [
            "apify",
            "actors",
            "start",
            actor_id,
            "--build",
            "latest",
            "--input-file",
            str(input_path),
            "--timeout",
            str(timeout),
            "--json",
        ],
        retries=1,
    )
    return extract_run_id(payload)


def _wait_for_run(run_id: str, timeout: int) -> dict:
    subprocess.run(
        ["apify", "runs", "wait", run_id, "--timeout", str(timeout), "--json"],
        check=True,
        text=True,
        capture_output=True,
    )
    return _run_json(["apify", "runs", "info", run_id, "--json"])


def _download_dataset(dataset_id: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("w", encoding="utf-8") as handle:
        subprocess.run(
            ["apify", "datasets", "get-items", dataset_id, "--format", "json"],
            check=True,
            text=True,
            stdout=handle,
        )


def execute_manifest(
    manifest: PilotManifest,
    out_dir: Path,
    *,
    concurrency: int = 8,
    attempts: int = 3,
    timeout: int = 3600,
) -> PilotManifest:
    for chunk in manifest.pending_chunks():
        run_id = chunk.run_id if chunk.status == "RUNNING" and chunk.run_id else None
        try:
            if run_id is None:
                run_id = _start_chunk(manifest.actor_id, chunk, out_dir, concurrency, attempts, timeout)
                manifest.mark_running(chunk.index, run_id)
                manifest.save()
            info = _wait_for_run(run_id, timeout)
            status = _find_key(info, "status")
            dataset_id = _find_key(info, "defaultDatasetId")
            if status != "SUCCEEDED" or not isinstance(dataset_id, str):
                raise RuntimeError(f"run {run_id} finished as {status!r} with dataset {dataset_id!r}")
            dataset_path = out_dir / f"chunk-{chunk.index:03d}.json"
            _download_dataset(dataset_id, dataset_path)
            manifest.mark_completed(
                chunk.index,
                run_id=run_id,
                dataset_id=dataset_id,
                dataset_path=str(dataset_path),
            )
            manifest.save()
        except Exception as exc:
            manifest.mark_failed(chunk.index, run_id=run_id, error=str(exc))
            manifest.save()
            raise
    return manifest


def _load_seed_file(path: Path) -> list[str]:
    if path.suffix.lower() == ".json":
        payload = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(payload, list):
            raise ValueError("JSON seed file must contain an array")
        return [item["url"] if isinstance(item, dict) else str(item) for item in payload]
    return [line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def _load_dataset_rows(path: Path) -> list[dict]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(payload, dict):
        return [payload]
    if isinstance(payload, list) and all(isinstance(item, dict) for item in payload):
        return [dict(item) for item in payload]
    raise ValueError(f"dataset {path} must contain a JSON object or array of objects")


def recovery_seeds_from_manifest(
    manifest_path: str | Path,
    *,
    statuses: set[int] | None = None,
) -> list[str]:
    retry_statuses = statuses or {999}
    raw = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    chunks = raw.get("chunks", []) if isinstance(raw, dict) else []
    rows: list[dict] = []
    for chunk in chunks:
        if not isinstance(chunk, dict) or chunk.get("status") != "SUCCEEDED":
            continue
        dataset_path = chunk.get("dataset_path")
        if not isinstance(dataset_path, str) or not dataset_path:
            continue
        rows.extend(_load_dataset_rows(Path(dataset_path)))

    successes: set[str] = set()
    for row in rows:
        try:
            status = int(row.get("status"))
        except (TypeError, ValueError):
            continue
        if status != 200 or row.get("fetched") is False:
            continue
        canonical = canonical_linkedin_url(str(row.get("url") or ""))
        if canonical:
            successes.add(canonical)

    recovery: list[str] = []
    seen: set[str] = set()
    for row in rows:
        try:
            status = int(row.get("status"))
        except (TypeError, ValueError):
            continue
        if status not in retry_statuses:
            continue
        canonical = canonical_linkedin_url(str(row.get("url") or ""))
        if not canonical or canonical in successes or canonical in seen:
            continue
        seen.add(canonical)
        recovery.append(canonical)
    return recovery


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Run a resumable depth-zero LinkedIn crawl on Apify")
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--seeds", type=Path)
    source.add_argument(
        "--recover-manifest",
        type=Path,
        help="Build one delayed recovery pass from unresolved HTTP 999 rows in a completed manifest",
    )
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--chunk-size", type=int, default=50)
    parser.add_argument("--concurrency", type=int, default=8)
    parser.add_argument("--attempts", type=int, default=3)
    parser.add_argument("--timeout", type=int, default=3600)
    parser.add_argument("--actor-id", default=DEFAULT_ACTOR_ID)
    args = parser.parse_args(argv)

    seeds = (
        recovery_seeds_from_manifest(args.recover_manifest)
        if args.recover_manifest is not None
        else _load_seed_file(args.seeds)
    )
    chunks = chunk_seeds(seeds, args.chunk_size)
    manifest_path = args.out / "manifest.json"
    manifest = PilotManifest.load(manifest_path, chunks, actor_id=args.actor_id)
    manifest.save()
    execute_manifest(
        manifest,
        args.out,
        concurrency=args.concurrency,
        attempts=args.attempts,
        timeout=args.timeout,
    )
    print(json.dumps({
        "manifest": str(manifest_path),
        "chunks": len(manifest.chunks),
        "seeds": sum(len(chunk.seeds) for chunk in manifest.chunks),
        "completed": sum(chunk.status == "SUCCEEDED" for chunk in manifest.chunks),
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

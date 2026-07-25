#!/usr/bin/env python3
from __future__ import annotations

import gzip
import sys
from pathlib import Path

root = Path(sys.argv[1] if len(sys.argv) > 1 else "dist")
files = [path for path in root.rglob("*") if path.is_file()]
raw = sum(path.stat().st_size for path in files)
gzipped = sum(len(gzip.compress(path.read_bytes(), compresslevel=9)) for path in files)
print(f"bundle files: {len(files)}")
print(f"bundle raw bytes: {raw}")
print(f"bundle independently gzipped bytes: {gzipped}")
if gzipped > 3 * 1024 * 1024:
    raise SystemExit("compressed bundle estimate exceeds the Workers Free-plan 3 MiB limit")

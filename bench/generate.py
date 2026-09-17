#!/usr/bin/env python3
"""Build a folder of test images for the README timings.

Stdlib only — PNGs are written by hand — so the benchmark reproduces on a clean
machine with nothing installed:

    python bench/generate.py 5000
    cargo build --release
    ./target/release/imgdupe bench/images --threshold 5

Every 10th picture is emitted three times: once at full size, once at half size
and once with a brightness shift. A correct run must group those three together
and leave the rest alone, so the benchmark measures the real work — hashing and
complete-linkage grouping — not a folder of identical files.
"""
from __future__ import annotations

import hashlib
import math
import shutil
import struct
import sys
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
IMAGES = HERE / "images"

BASE_W, BASE_H = 320, 240
DUPLICATE_EVERY = 10


def png_bytes(width: int, height: int, pixel) -> bytes:
    """Minimal 8-bit RGB PNG. `pixel(x, y) -> (r, g, b)`."""
    raw = bytearray()
    for y in range(height):
        raw.append(0)                                  # filter type 0 for the scanline
        for x in range(width):
            raw.extend(pixel(x, y))

    def chunk(tag: bytes, payload: bytes) -> bytes:
        return (struct.pack(">I", len(payload)) + tag + payload
                + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF))

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
            + chunk(b"IEND", b""))


GRID = 8


def pattern(seed: int, width: int, height: int, shift: int):
    """A deterministic picture, distinct per seed and stable under rescaling.

    The content is an 8x8 grid of brightness levels taken from a hash of the
    seed. That is deliberately the resolution a 64-bit dHash sees: smooth
    gradients would give thousands of "different" pictures the same hash and the
    benchmark would measure collisions instead of work.
    """
    cells = hashlib.blake2b(str(seed).encode(), digest_size=GRID * GRID).digest()

    def pixel(x: int, y: int):
        u, v = x / width, y / height
        cell = cells[min(int(v * GRID), GRID - 1) * GRID + min(int(u * GRID), GRID - 1)]
        level = max(0, min(255, cell + shift))
        if u < 0.25 and v < 0.25:
            return (10, 200, 40)                       # fixed block, survives rescaling
        return (level, level // 2, 255 - level)

    return pixel


def main() -> int:
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 5000
    if IMAGES.exists():
        shutil.rmtree(IMAGES)
    IMAGES.mkdir(parents=True)

    written = 0
    for seed in range(count):
        (IMAGES / f"img_{seed:06d}.png").write_bytes(
            png_bytes(BASE_W, BASE_H, pattern(seed, BASE_W, BASE_H, 0)))
        written += 1

        if seed % DUPLICATE_EVERY == 0:
            half_w, half_h = BASE_W // 2, BASE_H // 2
            (IMAGES / f"img_{seed:06d}_small.png").write_bytes(
                png_bytes(half_w, half_h, pattern(seed, half_w, half_h, 0)))
            (IMAGES / f"img_{seed:06d}_bright.png").write_bytes(
                png_bytes(BASE_W, BASE_H, pattern(seed, BASE_W, BASE_H, 18)))
            written += 2

    total = sum(p.stat().st_size for p in IMAGES.iterdir())
    groups = math.ceil(count / DUPLICATE_EVERY)
    print(f"{written} images, {total / 1e6:.0f} MB in {IMAGES}")
    print(f"expected: {groups} groups of 3")
    return 0


if __name__ == "__main__":
    sys.exit(main())

"""Persistent source/target labs for mutation runs.

A persistent Cargo target is useful only when source mtimes are fresh. If
mtimes are preserved, Cargo can reuse artifacts built from older or MUTATED
source: that is the 2026-09-17 stale-binary family documented in AGENTS.md.
The previous design rebuilt every dependency on every run (about 0.6 GiB
written and about 30 s) and left about 0.9 GB of orphaned files when killed.

This helper recreates the source tree while retaining the target, so workspace
crates rebuild from this run's source and dependencies remain reusable.
"""
from contextlib import contextmanager
from pathlib import Path
from typing import Iterable, Iterator, NamedTuple
import fcntl
import os
import shutil


class Lab(NamedTuple):
    src: Path
    target: Path


@contextmanager
def open_lab(root: Path, name: str, files: Iterable[str], trees: Iterable[str]) -> Iterator[Lab]:
    """Open an exclusively locked, fresh-source lab with a persistent target."""
    lab = Path(os.environ.get("ITA_MUTANT_LAB") or root / "target" / "mutant-lab") / name
    lab.mkdir(parents=True, exist_ok=True)
    lock_path = lab / ".lock"
    with lock_path.open("a+") as lock:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            lock.seek(0)
            holder = lock.read().strip() or "unknown"
            raise SystemExit(f"mutant lab {lab} is busy (holder pid {holder})")
        lock.seek(0)
        lock.truncate()
        lock.write(str(os.getpid()))
        lock.flush()
        src = lab / "src"
        shutil.rmtree(src, ignore_errors=True)
        src.mkdir(parents=True)
        for relative in files:
            source = root / relative
            destination = src / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(source, destination)
        for relative in trees:
            source = root / relative
            destination = src / relative
            shutil.copytree(
                source,
                destination,
                copy_function=shutil.copy,
                ignore=shutil.ignore_patterns("target"),
            )
        target = lab / "target"
        target.mkdir(parents=True, exist_ok=True)
        try:
            yield Lab(src=src, target=target)
        finally:
            fcntl.flock(lock.fileno(), fcntl.LOCK_UN)


def fresh_build(output: str, crate: str = "itaruby_semantic") -> bool:
    """Return whether Cargo rebuilt *crate* rather than reusing an artifact."""
    return f"Compiling {crate} v" in output

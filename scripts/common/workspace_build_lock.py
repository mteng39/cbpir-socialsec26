"""Cooperative shared lock for supported workspace build entrypoints."""

from __future__ import annotations

from contextlib import contextmanager
import fcntl
import os
from pathlib import Path
import stat


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
WORKSPACE_BUILD_LOCK = Path(
    os.environ.get(
        "CBPIR_WORKSPACE_BUILD_LOCK",
        REPOSITORY_ROOT / "target/locks/workspace-build.lock",
    )
)


def _acquire_shared(lock_path: Path = WORKSPACE_BUILD_LOCK) -> int:
    lock_path = Path(lock_path)
    lock_dir = lock_path.parent
    lock_dir.mkdir(parents=True, mode=0o700, exist_ok=True)
    directory = lock_dir.lstat()
    if stat.S_ISLNK(directory.st_mode) or not stat.S_ISDIR(directory.st_mode):
        raise RuntimeError(f"unsafe workspace build-lock directory: {lock_dir}")
    if lock_dir.resolve(strict=True) != lock_dir:
        raise RuntimeError(f"workspace build-lock directory is redirected: {lock_dir}")
    if directory.st_uid != os.geteuid() or stat.S_IMODE(directory.st_mode) != 0o700:
        raise RuntimeError(
            "workspace build-lock directory must be owned by this user with mode 0700"
        )

    flags = os.O_RDWR | os.O_CREAT | os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    previous_umask = os.umask(0o077)
    try:
        descriptor = os.open(lock_path, flags, 0o600)
    finally:
        os.umask(previous_umask)

    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode):
            raise RuntimeError(f"workspace build lock is not a regular file: {lock_path}")
        if opened.st_uid != os.geteuid() or stat.S_IMODE(opened.st_mode) != 0o600:
            raise RuntimeError(
                "workspace build lock must be owned by this user with mode 0600"
            )
        fcntl.flock(descriptor, fcntl.LOCK_SH)
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


@contextmanager
def shared_workspace_build_lock():
    """Hold the shared workspace-build lock for the wrapped build workflow."""
    descriptor = _acquire_shared()
    try:
        yield
    finally:
        os.close(descriptor)

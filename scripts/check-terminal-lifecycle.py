#!/usr/bin/env python3
"""Check terminal and mouse mode restoration after a normal workspace exit."""

import errno
import fcntl
import os
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path


def wait_for_workspace(
    master: int, process: subprocess.Popen[bytes], timeout: float
) -> bytearray:
    output = bytearray()
    deadline = time.monotonic() + timeout

    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"tuilane exited before rendering (status {process.returncode})")

        ready, _, _ = select.select([master], [], [], 0.1)
        if not ready:
            continue

        try:
            output.extend(os.read(master, 4096))
        except OSError as error:
            if error.errno == errno.EIO:
                continue
            raise

        if b"Overview" in output:
            return output

    raise TimeoutError("tuilane did not render its workspace in time")


def read_pending_output(master: int, output: bytearray) -> None:
    deadline = time.monotonic() + 1.0
    while time.monotonic() < deadline:
        ready, _, _ = select.select([master], [], [], 0.1)
        if not ready:
            return

        try:
            output.extend(os.read(master, 4096))
        except OSError as error:
            if error.errno == errno.EIO:
                return
            raise


def run_case(binary: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="tuilane-terminal-") as temporary:
        root = Path(temporary)
        data_directory = root / "data"
        config_path = root / "tuilane.toml"
        config_path.write_text("version = 1\n", encoding="utf-8")

        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))
        original_attributes = termios.tcgetattr(slave)
        environment = os.environ.copy()
        environment.update(
            {
                "TERM": "xterm-256color",
                "TUILANE_CONFIG": str(config_path),
                "TUILANE_DATA_DIR": str(data_directory),
            }
        )
        process = subprocess.Popen(
            [str(binary)],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=environment,
            close_fds=True,
        )

        try:
            output = wait_for_workspace(master, process, 10.0)
            os.write(master, b"q")

            process.wait(timeout=10.0)
            read_pending_output(master, output)
            restored_attributes = termios.tcgetattr(slave)
            if restored_attributes != original_attributes:
                raise RuntimeError("terminal settings were not restored after tuilane exited")
            mouse_modes = (b"1000", b"1002", b"1003", b"1015", b"1006")
            for mode in mouse_modes:
                if b"\x1b[?" + mode + b"h" not in output:
                    raise RuntimeError(f"mouse mode {mode.decode()} was not enabled")
                if b"\x1b[?" + mode + b"l" not in output:
                    raise RuntimeError(f"mouse mode {mode.decode()} was not disabled")
            if process.returncode != 0:
                raise RuntimeError(f"normal quit returned status {process.returncode}")
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=2.0)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            if master >= 0:
                os.close(master)
            os.close(slave)


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {Path(sys.argv[0]).name} PATH_TO_TUILANE", file=sys.stderr)
        return 2

    binary = Path(sys.argv[1]).resolve()
    if not binary.is_file():
        print(f"tuilane executable not found: {binary}", file=sys.stderr)
        return 2

    try:
        run_case(binary)
    except (OSError, RuntimeError, TimeoutError) as error:
        print(f"terminal lifecycle check failed: {error}", file=sys.stderr)
        return 1

    print("terminal settings and mouse capture restored after normal workspace exit")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

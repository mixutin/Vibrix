#!/usr/bin/env python3
"""Minimal test-only ivshmem server for one QEMU client and one receive vector."""

import argparse
import array
import os
from pathlib import Path
import socket
import struct
import time

ARMED = "VIBRIX: native IVSHMEM MSI-X armed"
FIRST = "VIBRIX: native MSI-X first delivery observed"
DISABLED = "VIBRIX: native MSI-X disabled"
DONE = "VIBRIX: native MSI-X post-disable silence verified"


def send_message(conn, value, fd=None):
    ancillary = []
    if fd is not None:
        ancillary = [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", [fd]))]
    sent = conn.sendmsg([struct.pack("<q", value)], ancillary)
    if sent != 8:
        raise RuntimeError(f"short ivshmem protocol write: {sent}")


def wait_marker(path, marker, deadline):
    while time.monotonic() < deadline:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except FileNotFoundError:
            text = ""
        if marker in text:
            return
        time.sleep(0.02)
    raise TimeoutError(f"timed out waiting for guest marker: {marker}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", required=True, type=Path)
    parser.add_argument("--debug-log", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()

    args.socket.unlink(missing_ok=True)
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    shm_fd = os.memfd_create("vibrix-ivshmem-test", os.MFD_CLOEXEC)
    os.ftruncate(shm_fd, 1 << 20)
    event_fd = os.eventfd(0, os.EFD_CLOEXEC)
    try:
        server.bind(str(args.socket))
        server.listen(1)
        print(f"ivshmem test server ready: {args.socket}", flush=True)
        server.settimeout(args.timeout)
        conn, _ = server.accept()
        with conn:
            print("qemu connected", flush=True)
            send_message(conn, 0)
            send_message(conn, 0)
            send_message(conn, -1, shm_fd)
            send_message(conn, 0, event_fd)
            print("protocol setup sent: version=0 id=0 vectors=1", flush=True)

            deadline = time.monotonic() + args.timeout
            for number, marker in [(1, ARMED), (2, FIRST), (3, DISABLED)]:
                wait_marker(args.debug_log, marker, deadline)
                os.write(event_fd, struct.pack("Q", 1))
                print(f"trigger {number}: eventfd += 1 after {marker}", flush=True)
            wait_marker(args.debug_log, DONE, deadline)
            print("guest verified post-disable silence", flush=True)
            while time.monotonic() < deadline:
                time.sleep(0.1)
    finally:
        server.close()
        os.close(event_fd)
        os.close(shm_fd)
        args.socket.unlink(missing_ok=True)


if __name__ == "__main__":
    main()

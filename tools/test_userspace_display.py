"""Verify the normal userspace boot through real RFB keys and QEMU pixels.

The independent glyph fixtures below check actual QMP screendumps. Host-only
oracle tests are not boot evidence. No generated image replaces guest output.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "build/qemu"
GLYPHS = {
    " ": [0] * 7,
    "v": [0, 0, 17, 17, 17, 10, 4],
    "i": [4, 0, 12, 4, 4, 4, 14],
    "b": [16, 16, 30, 17, 17, 17, 30],
    "r": [0, 0, 22, 25, 16, 16, 16],
    "x": [0, 0, 17, 10, 4, 10, 17],
    "$": [4, 15, 20, 14, 5, 30, 4],
    "e": [0, 0, 14, 17, 31, 16, 14],
    "c": [0, 0, 14, 17, 16, 17, 14],
    "h": [16, 16, 30, 17, 17, 17, 17],
    "o": [0, 0, 14, 17, 17, 17, 14],
    "n": [0, 0, 30, 17, 17, 17, 17],
    "f": [6, 9, 8, 28, 8, 8, 8],
    "t": [8, 8, 28, 8, 8, 9, 6],
}


def ppm(data: bytes) -> tuple[int, int, bytes]:
    match = re.match(rb"P6\s+(\d+)\s+(\d+)\s+255\n", data)
    if match is None:
        raise AssertionError("expected QEMU binary RGB PPM")
    width, height = map(int, match.groups())
    pixels = data[match.end():]
    if not (0 < width <= 8192 and 0 < height <= 8192):
        raise AssertionError("invalid framebuffer dimensions")
    if len(pixels) != width * height * 3:
        raise AssertionError("truncated or oversized framebuffer")
    return width, height, pixels


def require_text(data: bytes, text: str, row: int, column: int = 0) -> None:
    width, height, pixels = ppm(data)
    if row < 0 or column < 0 or 16 + (column + len(text)) * 12 > width or 16 + (row + 1) * 16 > height:
        raise AssertionError("text outside framebuffer")
    for index, character in enumerate(text):
        bitmap = GLYPHS[character]
        for y in range(16):
            for x in range(12):
                on = y < 14 and x < 10 and bitmap[y // 2] & (1 << (4 - x // 2))
                expected = bytes([0xE6 if on else 0x12]) * 3
                px, py = 16 + (column + index) * 12 + x, 16 + row * 16 + y
                offset = (py * width + px) * 3
                if pixels[offset:offset + 3] != expected:
                    raise AssertionError(f"pixels do not show {text!r}: cell {index}, pixel ({px},{py})")


def receive(client: socket.socket, count: int) -> bytes:
    data = bytearray()
    while len(data) < count:
        chunk = client.recv(count - len(data))
        if not chunk:
            raise RuntimeError("VNC disconnected during handshake")
        data.extend(chunk)
    return bytes(data)


def vnc_connect(port: int) -> socket.socket:
    client = socket.create_connection(("127.0.0.1", port), timeout=10)
    try:
        version = receive(client, 12)
        if version != b"RFB 003.008\n":
            raise RuntimeError(f"unexpected RFB version: {version!r}")
        client.sendall(version)
        count = receive(client, 1)[0]
        if count == 0 or 1 not in receive(client, count):
            raise RuntimeError("local QEMU did not offer RFB None security")
        client.sendall(b"\x01")
        if receive(client, 4) != b"\0\0\0\0":
            raise RuntimeError("RFB security negotiation failed")
        client.sendall(b"\x01")
        header = receive(client, 24)
        width, height = struct.unpack(">HH", header[:4])
        name_len = struct.unpack(">I", header[20:])[0]
        if not (0 < width <= 8192 and 0 < height <= 8192 and name_len <= 4096):
            raise RuntimeError("invalid RFB ServerInit")
        receive(client, name_len)
        return client
    except BaseException:
        client.close()
        raise


def key(client: socket.socket, keysym: int) -> None:
    # Actual RFB KeyEvent -> virtual PS/2 -> kernel TTY -> userspace read.
    for down in (1, 0):
        client.sendall(struct.pack(">BBHI", 4, down, 0, keysym))
        time.sleep(0.08)


class Qmp:
    def __init__(self, path: Path):
        self.client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.client.settimeout(15)
        self.client.connect(str(path))
        self.stream = self.client.makefile("rb")
        self.sequence = 0
        if "QMP" not in json.loads(self.stream.readline()):
            raise RuntimeError("missing QMP greeting")
        self.command("qmp_capabilities")

    def command(self, command: str, **arguments):
        self.sequence += 1
        request = {"execute": command, "id": self.sequence}
        if arguments:
            request["arguments"] = arguments
        self.client.sendall(json.dumps(request).encode() + b"\n")
        for _ in range(100):
            line = self.stream.readline()
            if not line:
                raise RuntimeError("QMP disconnected")
            reply = json.loads(line)
            if reply.get("id") == self.sequence:
                if "error" in reply:
                    raise RuntimeError(f"QMP {command}: {reply['error']}")
                return reply["return"]
        raise RuntimeError("QMP response missing")

    def capture(self, name: str) -> bytes:
        path = OUTPUT / name
        self.command("screendump", filename=str(path), format="ppm")
        return path.read_bytes()

    def close(self):
        self.stream.close()
        self.client.close()


def await_text(qmp: Qmp, name: str, expectations: list[tuple[str, int]]) -> None:
    deadline = time.monotonic() + 15
    last_error = None
    while time.monotonic() < deadline:
        data = qmp.capture(name)
        try:
            for text, row in expectations:
                require_text(data, text, row)
            return
        except AssertionError as error:
            last_error = error
            time.sleep(0.2)
    raise AssertionError(f"display did not reach expected state: {last_error}")


def integration() -> None:
    env = {name: value for name, value in os.environ.items() if not name.startswith("VIBRIX_")}
    OUTPUT.mkdir(parents=True, exist_ok=True)
    launch_log = OUTPUT.parent / "userspace-display-launch.log"
    serial = OUTPUT / "interactive-serial.log"
    debug = OUTPUT / "interactive-debugcon.log"
    for stale in (serial, debug, OUTPUT / "display-result.json"):
        stale.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="vibrix-display-") as scratch:
        qmp_path = Path(scratch) / "qmp.sock"
        env["VIBRIX_QEMU_QMP_SOCKET"] = str(qmp_path)
        qmp = None
        with launch_log.open("wb") as output:
            process = subprocess.Popen(
                ["bash", str(ROOT / "tools/run-qemu.sh"), "--vnc=97"],
                cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 240
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise RuntimeError(f"launcher exited: {process.returncode}")
                    if (
                        qmp_path.exists() and serial.exists() and debug.exists()
                        and "vibrix$ " in serial.read_text(errors="replace")
                        and "userspace TTY read waiting for keyboard" in debug.read_text(errors="replace")
                    ):
                        break
                    time.sleep(0.2)
                else:
                    raise RuntimeError("automatic userspace boot timed out")
                text = serial.read_text(errors="replace").replace("\r", "")
                for expected in ("Vibrix / vfetch", "Privilege: ring 3", "PID: 1", "Memory usage: unavailable"):
                    if expected not in text:
                        raise AssertionError(f"missing automatic userspace fact: {expected}")
                debug_text = debug.read_text(errors="replace")
                for marker in ("kernel Rust shell ELF loaded", "kernel userspace CR3 activated"):
                    if marker not in debug_text:
                        raise AssertionError(f"missing real boot milestone: {marker}")
                if "kernel console prompt ready" in debug_text:
                    raise AssertionError("default boot entered the kernel console")
                qmp = Qmp(qmp_path)
                await_text(qmp, "display-boot.ppm", [("vibrix$ ", 15)])
                require_text((OUTPUT / "display-boot.ppm").read_bytes(), "vfetch", 2, 21)
                with vnc_connect(5997) as client:
                    for character in "echo vnx":
                        key(client, ord(character))
                    key(client, 0xFF08)
                    key(client, ord("c"))
                    await_text(qmp, "display-edit.ppm", [("vibrix$ echo vnc", 15)])
                    key(client, 0xFF0D)
                    await_text(qmp, "display-command.ppm", [("vnc", 16), ("vibrix$ ", 17)])
                    if "\nvnc\nvibrix$ " not in serial.read_text(errors="replace").replace("\r", ""):
                        raise AssertionError("serial mirror lost the real shell response")

                    def command(line: str, expected: str = "") -> str:
                        offset = serial.stat().st_size
                        for character in line:
                            key(client, ord(character))
                        key(client, 0xFF0D)
                        deadline = time.monotonic() + 15
                        while time.monotonic() < deadline:
                            reply = serial.read_bytes()[offset:].decode(errors="replace").replace("\r", "")
                            if "vibrix$ " in reply:
                                if expected not in reply:
                                    raise AssertionError(f"{line[:48]!r} missing {expected!r}: {reply!r}")
                                return reply
                            if process.poll() is not None:
                                raise AssertionError("guest exited during command test")
                            time.sleep(0.05)
                        raise AssertionError(f"no userspace prompt after {line[:48]!r}")

                    command("help", "vfetch (aliases: neofetch fastfetch)")
                    for alias in ("vfetch", "neofetch", "fastfetch"):
                        command(alias, "Privilege: ring 3")
                    command("uname", "\nVibrix\n")
                    command("uname -a", "Vibrix x86_64 native Rust userspace")
                    command("pid", "\n1\n")
                    command("vfetch -x", "sh: command failed")
                    command("notacommand", "sh: unknown command")
                    command("a" * 270, "sh: unknown command")
                    command("echo recovered", "\nrecovered\n")
                    command("cat /welcome", "Vibrix bootstrap filesystem: files live in RAM until reboot.")
                    command("mkdir /tmp/session")
                    command("cd /tmp/session")
                    command("pwd", "\n/tmp/session\n")
                    command("cp /welcome copy")
                    command("cat copy", "Vibrix bootstrap filesystem: files live in RAM until reboot.")
                    command("clear", "\x0c")
                    # Form feed is the bounded native clear operation: one
                    # redraw, cursor home, no ANSI parser or repeated scrolling.
                    await_text(
                        qmp,
                        "display-clear.ppm",
                        [("vibrix$ ", 0)] + [(" " * 80, row) for row in range(1, 30)],
                    )
                    command("echo vnc", "\nvnc\n")
                    await_text(
                        qmp,
                        "display-final.ppm",
                        [("vibrix$ echo vnc", 0), ("vnc", 1), ("vibrix$ ", 2)],
                    )
                evidence = {
                    "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    "result": "passed",
                    "input": "real localhost RFB keyboard events",
                    "pixels": "automatic vfetch, prompt, pre-Enter editing, response, clear and scrolling",
                }
                (OUTPUT / "display-result.json").write_text(json.dumps(evidence, indent=2) + "\n")
                print(json.dumps(evidence, indent=2))
                print("PASS: automatic Ring-3 shell, vfetch and real VNC keyboard editing")
                print("PASS: aliases, argument errors, full-line recovery and RAM file commands")
                print("PASS: cleared cells, scrolling, command response and prompt are guest pixels")
            except BaseException:
                if qmp is not None:
                    try:
                        qmp.capture("display-failure.ppm")
                    except Exception:
                        pass
                raise
            finally:
                if qmp is not None:
                    qmp.close()
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
                print(launch_log.read_text(errors="replace"))
                for name in ("interactive-debugcon.log", "interactive-serial.log"):
                    path = OUTPUT / name
                    if path.exists():
                        print(f"--- {name} ---\n{path.read_text(errors='replace')}")


class PixelOracleTests(unittest.TestCase):
    def test_rejects_truncated_image(self):
        with self.assertRaises(AssertionError):
            ppm(b"P6\n64 64\n255\n" + bytes(10))

    def test_blank_screen_cannot_satisfy_prompt(self):
        data = b"P6\n320 200\n255\n" + bytes([0x12]) * (320 * 200 * 3)
        with self.assertRaises(AssertionError):
            require_text(data, "vibrix$ ", 1)

    def test_binary_whitespace_pixel_is_not_stripped(self):
        self.assertEqual(ppm(b"P6\n1 1\n255\n\n\x20\x09"), (1, 1, b"\n\x20\x09"))


if __name__ == "__main__":
    if sys.argv[1:] == ["--unit"]:
        unittest.main(argv=[sys.argv[0]])
    elif sys.argv[1:]:
        sys.exit("Usage: python3 tools/test_userspace_display.py [--unit]")
    else:
        integration()

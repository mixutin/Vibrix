"""Boot the exact native desktop; require real pixel, VFS and PS/2 evidence.

RFB is only keyboard transport. QMP injects physical relative mouse/button
input, never writes guest memory. Every screenshot comes from QEMU screendump.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

from test_userspace_display import Qmp, GLYPHS, OUTPUT, ROOT, key, ppm, vnc_connect

FONT = {**GLYPHS,
    "C": [14, 17, 16, 16, 16, 17, 14], "H": [17, 17, 17, 31, 17, 17, 17],
    "R": [30, 17, 17, 30, 20, 18, 17], "O": [14, 17, 17, 17, 17, 17, 14],
    "M": [17, 27, 21, 21, 17, 17, 17], "I": [14, 4, 4, 4, 4, 4, 14],
    "U": [17, 17, 17, 17, 17, 17, 14], ":": [0, 12, 12, 0, 12, 12, 0],
    "N": [17, 25, 25, 21, 19, 19, 17], "T": [31, 4, 4, 4, 4, 4, 4],
    "P": [30, 17, 17, 30, 16, 16, 16], "E": [31, 16, 16, 30, 16, 16, 31],
    "D": [30, 17, 17, 17, 17, 17, 30],
    "d": [1, 1, 15, 17, 17, 17, 15],
    "s": [0, 0, 15, 16, 14, 1, 30],
    "k": [16, 16, 18, 20, 24, 20, 18],
    "p": [0, 0, 30, 17, 30, 16, 16],
    "a": [0, 0, 14, 1, 15, 17, 15],
    "u": [0, 0, 17, 17, 17, 19, 13],
    "l": [12, 4, 4, 4, 4, 4, 14],
}
FG = bytes.fromhex("dbe7f3")
BG = bytes.fromhex("101925")
ACCENT = bytes.fromhex("5ea5ed")


def require_text(data: bytes, text: str, x: int, y: int, foreground: bytes = FG) -> None:
    width, height, pixels = ppm(data)
    if x < 0 or y < 0 or x + len(text) * 12 > width or y + 16 > height:
        raise AssertionError("text outside captured display")
    for column, character in enumerate(text):
        bitmap = FONT[character]
        for py in range(16):
            for px in range(12):
                on = py < 14 and px < 10 and bitmap[py // 2] & (1 << (4 - px // 2))
                offset = ((y + py) * width + x + column * 12 + px) * 3
                if pixels[offset:offset + 3] != (foreground if on else BG):
                    raise AssertionError(f"missing real text {text!r} at ({x},{y}), cell {column}")


def await_pixels(qmp: Qmp, name: str, check, timeout: float = 20) -> bytes:
    deadline = time.monotonic() + timeout
    error = None
    while time.monotonic() < deadline:
        data = qmp.capture(name)
        try:
            check(data)
            return data
        except AssertionError as failure:
            error = failure
            time.sleep(0.15)
    raise AssertionError(f"desktop pixel check timed out: {error}")


def require_pointer(data: bytes, x: int, y: int) -> None:
    width, height, pixels = ppm(data)
    if not (0 <= x <= width - 12 and 0 <= y <= height - 18):
        raise AssertionError("cursor outside display")
    for row in range(16):
        size = min(row // 2 + 1, 10)
        for column in range(size):
            expected = b"\xff\xff\xff" if 0 < column < size - 1 else bytes.fromhex("050910")
            offset = ((y + row) * width + x + column) * 3
            if pixels[offset:offset + 3] != expected:
                raise AssertionError(f"cursor has not reached ({x},{y})")


class Pointer:
    def __init__(self, qmp: Qmp, width: int, height: int):
        self.qmp = qmp
        self.width, self.height = width, height
        self.x, self.y = width - 24, 44

    def move(self, dx: int, dy: int) -> None:
        target_x = min(max(self.x + dx, 0), self.width - 12)
        target_y = min(max(self.y + dy, 0), self.height - 18)
        # A large jump can overflow the 16-byte PS/2 FIFO before the guest
        # runs. Send one packet, then acknowledge real cursor pixels.
        while (self.x, self.y) != (target_x, target_y):
            sx = min(max(target_x - self.x, -100), 100)
            sy = min(max(target_y - self.y, -100), 100)
            self.qmp.command("input-send-event", events=[
                {"type": "rel", "data": {"axis": "x", "value": sx}},
                {"type": "rel", "data": {"axis": "y", "value": sy}},
            ])
            self.x += sx
            self.y += sy
            await_pixels(self.qmp, "desktop-pointer.ppm", lambda data: require_pointer(data, self.x, self.y))

    def button(self, down: bool) -> None:
        self.qmp.command("input-send-event", events=[{"type": "btn", "data": {"button": "left", "down": down}}])
        time.sleep(0.3)


def integration() -> None:
    env = {k: v for k, v in os.environ.items() if not k.startswith("VIBRIX_")}
    OUTPUT.mkdir(parents=True, exist_ok=True)
    launch_log = OUTPUT.parent / "desktop-display-launch.log"
    result_path = OUTPUT / "desktop-result.json"
    result_path.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="vibrix-desktop-") as directory:
        qmp_path = Path(directory) / "qmp.sock"
        env["VIBRIX_QEMU_QMP_SOCKET"] = str(qmp_path)
        qmp = client = None
        with launch_log.open("wb") as log:
            process = subprocess.Popen(["bash", str(ROOT / "tools/run-qemu.sh"), "--desktop", "--vnc=96"],
                cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                deadline = time.monotonic() + 180
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise RuntimeError(f"desktop launcher exited {process.returncode}; see {launch_log}")
                    serial = OUTPUT / "interactive-serial.log"
                    if qmp_path.exists() and serial.exists() and b"VIBRIX: desktop ready" in serial.read_bytes():
                        break
                    time.sleep(0.25)
                else:
                    for path in [launch_log, OUTPUT / "interactive-debugcon.log", OUTPUT / "interactive-serial.log"]:
                        if path.exists(): print(path, path.read_text(errors="replace")[-6000:], flush=True)
                    raise AssertionError("native desktop never became ready")
                debug = (OUTPUT / "interactive-debugcon.log").read_bytes()
                serial_bytes = serial.read_bytes()
                assert b"VIBRIX: desktop CPL3 graphics/input boundary verified" in serial_bytes
                assert b"VIBRIX: desktop PS2 pointer ready" in debug
                qmp = Qmp(qmp_path)
                mice = qmp.command("query-mice")
                assert any(m["name"] == "QEMU PS/2 Mouse" and m["current"] and not m["absolute"] for m in mice)
                initial = qmp.capture("desktop-boot.ppm")
                width, height, _ = ppm(initial)
                client = vnc_connect(5996)
                pointer = Pointer(qmp, width, height)

                def command(text: str) -> None:
                    for char in text:
                        key(client, 0xFF0D if char == "\n" else ord(char))

                command("clear\n")
                await_pixels(qmp, "desktop-terminal.ppm", lambda data: require_text(data, "vibrix$", 48, 100))
                command("echo desktop\n")
                await_pixels(qmp, "desktop-terminal.ppm", lambda data: require_text(data, "desktop", 48, 116))
                command("echo native > /note\ncat /note\n")
                await_pixels(qmp, "desktop-terminal.ppm", lambda data: require_text(data, "native", 48, 164))
                assert b"native\n" in serial.read_bytes()
                key(client, 0xFFBF)  # F2: independent file view of the same VFS.
                note_index = None

                def find_note(data: bytes) -> None:
                    nonlocal note_index
                    for index in range(16):
                        try:
                            require_text(data, "note", 64, 152 + index * 20, ACCENT if index == 0 else FG)
                            note_index = index
                            return
                        except AssertionError:
                            pass
                    raise AssertionError("terminal-created note absent from file browser")

                await_pixels(qmp, "desktop-files.ppm", find_note)
                assert note_index is not None
                target_x, target_y = 96, 160 + note_index * 20
                pointer.move(target_x - (width - 24), target_y - 44)
                pointer.button(True)
                pointer.button(False)
                await_pixels(qmp, "desktop-files.ppm", lambda data: (require_text(data, "native", 300, 152), require_pointer(data, pointer.x, pointer.y)))
                # Drag the actual title bar. A synthetic changed screenshot is
                # insufficient: real file bytes must move by the same offset.
                pointer.move(100 - target_x, 68 - target_y)
                pointer.button(True)
                pointer.move(16, 16)
                pointer.button(False)
                await_pixels(qmp, "desktop-drag.ppm", lambda data: (require_text(data, "native", 316, 168), require_pointer(data, pointer.x, pointer.y)))
                key(client, 0xFFC8)  # F11: maximize without discarding state.
                await_pixels(qmp, "desktop-maximized.ppm", lambda data: (require_text(data, "native", 276, 140), require_pointer(data, pointer.x, pointer.y)))
                key(client, 0xFFC8)
                key(client, 0xFFC1)  # F4: explicitly labelled port-status panel.
                await_pixels(qmp, "desktop-chromium-status.ppm", lambda data: require_text(data, "CHROMIUM: NOT PORTED", 68, 124, ACCENT))
                key(client, 0xFFBE)
                command("clear\necho restored\n")
                await_pixels(qmp, "desktop-restored.ppm", lambda data: require_text(data, "restored", 64, 132))
                for path in [OUTPUT / "interactive-debugcon.log", serial]:
                    log_bytes = path.read_bytes().lower()
                    assert b"page fault" not in log_bytes and b"desktop panic" not in log_bytes and b"desktop: render failed" not in log_bytes
                result = {"status": "passed", "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    "worktree_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
                    "resolution": [width, height], "cpl3": True, "bad_pointer_rejected": True,
                    "real_terminal_commands": True, "real_vfs_file_preview": True,
                    "physical_ps2_mouse_click_drag": True, "maximize_restore": True,
                    "chromium_running": False, "screenshots": sorted(p.name for p in OUTPUT.glob("desktop-*.ppm"))}
                result_path.write_text(json.dumps(result, indent=2) + "\n")
                print(json.dumps(result, indent=2), flush=True)
            finally:
                if client is not None: client.close()
                if qmp is not None: qmp.close()
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try: process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)


class OracleTests(unittest.TestCase):
    def test_blank_framebuffer_never_proves_a_terminal(self):
        image = b"P6\n640 480\n255\n" + BG * (640 * 480)
        with self.assertRaises(AssertionError): require_text(image, "native", 48, 100)

    def test_truncated_and_out_of_bounds_frames_fail(self):
        with self.assertRaises(AssertionError): require_text(b"P6\n640 480\n255\n", "native", 48, 100)
        image = b"P6\n640 480\n255\n" + BG * (640 * 480)
        with self.assertRaises(AssertionError): require_text(image, "native", 630, 100)


if __name__ == "__main__":
    if "--unit" in sys.argv: unittest.main(argv=[sys.argv[0]])
    else: integration()

"""Validate paired, actual QEMU logs; fixture tests do not establish guest behavior."""

import argparse
import pathlib
import re
import sys

BASE = 0xFFFFD00000000000
EXPECTED = {
    "write": (BASE, 3),
    "unmap": (BASE, 0),
    "guard": (BASE + 8192, 0),
    "nx": (BASE, 17),
}
NATIVE = "managed VM: dynamic map/protect/reclaim, zeroed regions and reserved guards verified"
FAULT = re.compile(
    r"kernel #PF cr2=(0x[0-9a-f]+) rip=(0x[0-9a-f]+) error=(0x[0-9a-f]+) "
    r"present=(true|false) write=(true|false) user=(true|false) "
    r"reserved=(true|false) exec=(true|false)"
)


def validate(probe: str, debug: str, serial: str) -> None:
    if probe not in EXPECTED:
        raise ValueError("unknown probe")
    debug_lines = debug.replace("\r", "").splitlines()
    serial_lines = serial.replace("\r", "").splitlines()
    combined = "\n".join(debug_lines + serial_lines).lower()
    for forbidden in (
        "kernel panic", "kernel double fault", "kernel general protection fault",
        "kernel #df", "kernel #gp", "fault probe returned", "kernel console prompt ready", "vibrix>",
    ):
        if forbidden in combined:
            raise ValueError(f"unexpected guest outcome: {forbidden}")
    markers = [
        "VIBRIX: ExitBootServices succeeded",
        "VIBRIX: kernel entry after ExitBootServices",
        "VIBRIX: kernel IDT installed",
        "VIBRIX: kernel managed VM native verified",
        f"VIBRIX: managed VM {probe} fault armed",
        "VIBRIX: kernel page fault diagnostic",
    ]
    positions = []
    for marker in markers:
        if debug_lines.count(marker) != 1:
            raise ValueError(f"missing or duplicate kernel marker: {marker}")
        positions.append(debug_lines.index(marker))
    if positions != sorted(positions):
        raise ValueError("guest markers are out of execution order")
    address, error = EXPECTED[probe]
    if serial_lines.count(NATIVE) != 1:
        raise ValueError("missing or duplicate independent native COM1 evidence")
    armed = f"managed VM probe={probe} address={address:#x}"
    if serial_lines.count(armed) != 1:
        raise ValueError("wrong/missing COM1 probe identity or address")
    fault_lines = [line for line in serial_lines if "kernel #PF" in line]
    if len(fault_lines) != 1:
        raise ValueError("expected exactly one native page-fault record")
    match = FAULT.fullmatch(fault_lines[0])
    if match is None:
        raise ValueError("malformed page-fault record")
    cr2, rip, actual_error = (int(value, 16) for value in match.groups()[:3])
    if cr2 != address or actual_error != error:
        raise ValueError(f"wrong CPU fault: CR2={cr2:#x}, error={actual_error:#x}")
    if rip < 0xFFFF800000000000 or (probe == "nx" and rip != address):
        raise ValueError("unexpected fault instruction pointer")
    flags = tuple(value == "true" for value in match.groups()[3:])
    expected_flags = tuple(bool(error & (1 << bit)) for bit in range(5))
    if flags != expected_flags:
        raise ValueError("decoded CPU fault bits do not match the raw error")
    if not serial_lines.index(NATIVE) < serial_lines.index(armed) < serial_lines.index(fault_lines[0]):
        raise ValueError("COM1 evidence is out of execution order")


def read_log(path: pathlib.Path) -> str:
    if not path.is_file() or path.stat().st_size > 2 * 1024 * 1024:
        raise ValueError(f"missing or oversized log: {path}")
    return path.read_text(encoding="utf-8", errors="replace")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", choices=EXPECTED)
    parser.add_argument("debug_log", type=pathlib.Path)
    parser.add_argument("serial_log", type=pathlib.Path)
    args = parser.parse_args()
    try:
        validate(args.probe, read_log(args.debug_log), read_log(args.serial_log))
    except (OSError, ValueError) as error:
        print(f"managed VM evidence rejected: {error}", file=sys.stderr)
        return 1
    print(f"managed VM {args.probe}: exact CPU fault and paired guest evidence verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

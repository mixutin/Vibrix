"""Parser-only fixtures; these are never substituted for actual QEMU output."""

import importlib.util
import pathlib
import unittest

path = pathlib.Path(__file__).resolve().parents[1] / "check-managed-vm.py"
spec = importlib.util.spec_from_file_location("managed_evidence", path)
evidence = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evidence)


def fixture(probe):
    address, error = evidence.EXPECTED[probe]
    rip = address if probe == "nx" else 0xFFFFFFFF80001234
    bits = [str(bool(error & (1 << bit))).lower() for bit in range(5)]
    debug = "\n".join([
        "VIBRIX: ExitBootServices succeeded",
        "VIBRIX: kernel entry after ExitBootServices",
        "VIBRIX: kernel IDT installed",
        "VIBRIX: kernel managed VM native verified",
        f"VIBRIX: managed VM {probe} fault armed",
        "VIBRIX: kernel page fault diagnostic",
    ])
    serial = "\n".join([
        evidence.NATIVE,
        f"managed VM probe={probe} address={address:#x}",
        f"kernel #PF cr2={address:#x} rip={rip:#x} error={error:#x} "
        f"present={bits[0]} write={bits[1]} user={bits[2]} reserved={bits[3]} exec={bits[4]}",
    ])
    return debug, serial


class EvidenceTests(unittest.TestCase):
    def test_exact_records_and_crlf(self):
        for probe in evidence.EXPECTED:
            with self.subTest(probe=probe):
                debug, serial = fixture(probe)
                evidence.validate(probe, debug, serial)
                evidence.validate(probe, debug.replace("\n", "\r\n"), serial.replace("\n", "\r\n"))

    def test_mutated_cpu_fields_are_rejected(self):
        debug, serial = fixture("write")
        changes = [
            ("cr2=0xffffd00000000000", "cr2=0xffffd00000001000"),
            ("error=0x3", "error=0x30"),
            ("write=true", "write=false"),
            ("user=false", "user=true"),
            ("reserved=false", "reserved=true"),
            ("rip=0xffffffff80001234", "rip=0x1234"),
        ]
        for before, after in changes:
            with self.subTest(change=before), self.assertRaises(ValueError):
                evidence.validate("write", debug, serial.replace(before, after))

    def test_missing_duplicate_out_of_order_and_unpaired_evidence(self):
        debug, serial = fixture("unmap")
        mutations = [
            (debug.replace("VIBRIX: kernel IDT installed", ""), serial),
            (debug + "\nVIBRIX: kernel page fault diagnostic", serial),
            ("\n".join(reversed(debug.splitlines())), serial),
            (debug, serial + "\n" + serial.splitlines()[-1]),
            (debug, serial.replace(evidence.NATIVE, "")),
            (debug, "\n".join(reversed(serial.splitlines()))),
            (debug, fixture("guard")[1]),
        ]
        for changed_debug, changed_serial in mutations:
            with self.subTest(debug=changed_debug), self.assertRaises(ValueError):
                evidence.validate("unmap", changed_debug, changed_serial)

    def test_wrong_guest_outcomes_cannot_satisfy_a_fault(self):
        debug, serial = fixture("guard")
        for suffix in ["kernel panic", "kernel #GP", "vibrix> ", "managed VM fault probe returned"]:
            with self.subTest(suffix=suffix), self.assertRaises(ValueError):
                evidence.validate("guard", debug, serial + "\n" + suffix)

    def test_nx_requires_instruction_fetch_bit_and_exact_fault_rip(self):
        debug, serial = fixture("nx")
        for changed in [
            serial.replace("exec=true", "exec=false"),
            serial.replace("rip=0xffffd00000000000", "rip=0xffffffff80001234"),
            serial.replace("error=0x11", "error=0x1"),
        ]:
            with self.assertRaises(ValueError):
                evidence.validate("nx", debug, changed)


if __name__ == "__main__":
    unittest.main()

# Vibrix Hardware Targets

Vibrix tracks real machines as explicit hardware targets.

Raw inventories belong in `targets/raw/` and are intentionally ignored because they may contain serial numbers, UUIDs and other machine identifiers.

## Target 001

Target 001 is the first physical reference machine for Vibrix. Generate its inventory from the repository root:

```bash
sudo apt update
sudo apt install -y pciutils usbutils dmidecode efibootmgr nvme-cli
chmod +x tools/collect-target.sh
./tools/collect-target.sh
```

The collector writes `targets/raw/target-001.txt`. Review and sanitize the raw profile before committing a permanent target definition.

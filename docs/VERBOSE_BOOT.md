# Verbose boot mode

Vibrix keeps additional boot diagnostics explicitly opt-in through the kernel
`verbose-boot` feature. The default boot path is unchanged and does not emit
the verbose markers.

When enabled, the kernel reports bounded boot metadata after COM1 is available
and a summary of native PCI discovery/binding after enumeration. It does not
dump memory contents, user data, secrets, hardware serial numbers, or arbitrary
firmware tables.

The dedicated evidence workflow boots both modes. The normal boot must contain
no verbose markers; a second build with `qemu-debugcon,verbose-boot` must emit
the expected serial and debugcon markers.

This is an early compile-time boot-mode control. A persistent/user-selectable
bootloader setting and richer runtime log-level configuration remain separate.

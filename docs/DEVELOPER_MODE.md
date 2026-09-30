# Standard and developer operational modes

Vibrix keeps diagnostic/developer operation explicit instead of silently
enabling it in the ordinary build.

The kernel feature `developer-mode` is opt-in. The normal kernel build does
not enable it and reports `VIBRIX: operational mode standard`. An explicit
developer build reports `VIBRIX: operational mode developer`.

Fault-injection and low-level diagnostic probes that deliberately crash, trap,
mutate protected mappings, or exercise raw PCI interrupt paths depend on
`developer-mode`. `verbose-boot` also requires developer mode. This prevents
those facilities from being selected without making the developer boundary
visible in the compiled feature graph.

For interactive QEMU development, `tools/run-qemu.sh --developer` boots the
normal Ring 3 shell with developer mode and verbose diagnostics. The default
launcher remains standard mode.

This separation is a policy boundary, not a claim that standard mode has
completed every hardening milestone. ASLR, IOMMU enforcement, persistent-root
security and several other roadmap controls remain separate work.

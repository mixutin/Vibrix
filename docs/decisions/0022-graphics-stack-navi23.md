# ADR 0022: graphics stack, compositor boundary, and Navi 23 modesetting research

- **Status:** Accepted
- **Date:** 2026-09-29
- **Authoring AI:** GPT-5.6 Sol
- **Roadmap:** M13 — graphics architecture; compositor/display-server design; Navi 23 modesetting research
- **Supersedes:** None
- **Compatibility:** No current syscall, VFS, framebuffer, PCI, USB, package, or on-disk ABI change.

## Decision

Vibrix will use a layered graphics architecture with three explicit ownership
boundaries:

1. **Kernel display driver / modesetting layer**
   - owns display hardware, MMIO, interrupts, firmware-facing state, scanout
     resources, and mode validation;
   - exposes capability/state objects rather than raw device registers to
     userspace;
   - performs atomic validate-then-commit transitions for connector, mode,
     plane, scanout-buffer and cursor state;
   - retains the existing firmware framebuffer only as a bootstrap/fallback
     path until native modesetting is proven.

2. **Userspace compositor/display server**
   - is the single session owner of display presentation and input routing;
   - receives client-owned surfaces/buffers through a bounded object protocol;
   - chooses placement, z-order, focus, damage and frame scheduling;
   - presents one composed result through the kernel display API;
   - does not map arbitrary GPU MMIO or own kernel interrupt state.

3. **Applications / clients**
   - render into explicitly shared buffers;
   - commit surface state asynchronously;
   - never choose physical connectors, CRTCs, planes or scanout addresses
     directly.

This intentionally follows the useful separation visible in upstream DRM/KMS,
AMD Display Core and Wayland while defining a Vibrix-native API rather than
claiming Linux or Wayland ABI compatibility.

## Kernel display object model

The first native display contract should model:

- **Device** — one bound graphics/display device.
- **Connector** — a physical/logical display sink and its detected modes.
- **Encoder/link** — hardware path connecting a display pipeline to a sink.
- **CRTC/timing generator** — one scanout timing owner.
- **Plane** — a scanout image source with format, dimensions and position.
- **Framebuffer/scanout buffer** — validated memory object referenced by a
  plane, never a naked userspace physical address.
- **Atomic state** — an immutable proposed configuration validated as a whole
  before hardware mutation.

The initial implementation may support exactly one connector, one timing
generator and one primary plane. The object model must still make unsupported
multiplicity explicit instead of baking a single monitor into the ABI.

A successful commit must either publish the complete validated state or leave
the previous state active. Partial programming is not a valid userspace-visible
result.

## Buffer and security boundary

Early userspace rendering should begin with CPU-writable linear buffers. The
kernel validates dimensions, stride, byte length, pixel format, ownership and
mapping before a buffer can become scanout-visible.

Future GPU-accelerated buffers may be added behind the same ownership model, but
applications will not receive arbitrary BAR mappings, display-register access,
or unrestricted physical addresses. Buffer sharing needs an explicit lifetime
and synchronization contract before acceleration is exposed.

This design keeps M11 IOMMU, W^X, isolation and later acceleration work
separate. It does not claim DMA isolation today.

## Compositor/display-server design

The compositor is a normal privileged userspace service, not part of the
kernel. It owns one display-session endpoint and exposes an asynchronous
object-oriented protocol with bounded handles for:

- client connection/session;
- surface;
- attached buffer;
- damage region;
- output;
- keyboard/pointer seat;
- frame callback.

The minimum transaction is:

1. client creates a surface;
2. client attaches a buffer and damage;
3. client commits;
4. compositor reads the new committed surface state;
5. compositor composites or directly presents it;
6. compositor later releases the buffer / emits a frame-complete event.

Clients do not know or control global hardware topology. Input focus and output
placement are compositor policy.

The first implementation may use shared-memory software rendering only. A
Wayland-compatible wire protocol is **not** required by this decision; the
architecture is inspired by the same client-buffer/compositor ownership model.

## Navi 23 research conclusion

Target 001's Radeon RX 6600 XT is a Navi 23-family RDNA2 device. Upstream AMD
display code places this generation in the Navi-family Display Core Next path
and demonstrates that modern AMD display bring-up is not a single framebuffer
BAR write: it spans device/IP discovery, firmware/microcontroller state,
resource construction, link/connector discovery, mode validation, clocks,
memory-watermark policy and atomic stream/plane programming.

The public AMD/Linux documentation is sufficient to establish the architecture
and investigation path, but not sufficient to justify a hand-written,
register-by-register Vibrix modeset sequence from secondary guesses.

Therefore the native Navi 23 implementation must:

1. identify the exact PCI device/revision and display IP version at runtime;
2. inventory BARs and required firmware before writes;
3. document every programmed register/firmware interface against an upstream
   AMD source or published specification;
4. bring up one conservative connector/mode/primary-plane path first;
5. preserve firmware framebuffer fallback until the native mode is verified;
6. validate on QEMU/virtual hardware only where that hardware actually models
   the relevant interface, and require Target 001 evidence for the physical
   Navi 23 checkbox/claims.

No native modesetting checkbox is satisfied by this research.

## Primary sources reviewed

Reviewed 2026-09-29:

- Linux kernel AMD Display Core documentation:
  https://docs.kernel.org/gpu/amdgpu/display/index.html
- Linux kernel Display Core Next overview:
  https://docs.kernel.org/gpu/amdgpu/display/dcn-overview.html
- Linux kernel DC programming model:
  https://docs.kernel.org/gpu/amdgpu/display/programming-model-dcn.html
- Linux DRM/KMS modesetting helpers and atomic model:
  https://docs.kernel.org/gpu/drm-kms-helpers.html
- Upstream AMD display ASIC-family identifiers:
  https://github.com/torvalds/linux/blob/master/drivers/gpu/drm/amd/display/include/dal_asic_id.h
- Upstream AMDGPU ASIC names:
  https://github.com/torvalds/linux/blob/master/drivers/gpu/drm/amd/amdgpu/amdgpu_device.c
- Wayland architecture / compositor-client buffer model:
  https://wayland.freedesktop.org/architecture.html
  https://wayland.freedesktop.org/docs/book/Protocol.html

## Consequences

### Positive

- Kernel hardware ownership stays separate from desktop/window policy.
- The first software framebuffer path can evolve into native scanout without
  changing application rendering ownership.
- Atomic state gives a fail-closed place for mode validation.
- Navi 23 work has a documented upstream research path instead of speculative
  register writes.

### Costs

- A real graphical desktop requires a new kernel display API, shared-buffer
  lifetime rules, compositor IPC and substantially more AMD hardware work.
- Acceleration cannot be safely exposed until memory ownership, synchronization
  and isolation are designed.

## Roadmap boundary

This decision completes the M13 **graphics architecture** and
**compositor/display-server design** tasks, and records the requested
**Navi 23 modesetting research**.

It does **not** complete framebuffer userspace API, native modesetting,
acceleration, GUI toolkit, HDA/USB audio, Target 001 display validation or any
Wayland compatibility claim.

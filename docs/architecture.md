# Architecture and ownership

The foundation is a standalone Ports and Adapters workspace. Dependencies point inward: `edf-core` defines universal domain records, lifecycle and reset behavior, and port traits; adapters depend on those interfaces; a device firmware project later composes the adapters with its own domain, board, transport and runtime policy. Every Rust library supports `no_std` production builds. Host tests use mocks and test-only `std` support.

```text
consumer firmware and dashboard (later integration)
  ├─ device state, actions, endpoint metadata, pins, TCP/server, Wi-Fi runtime
  ├─ edf-gpio / edf-ota / edf-logger / edf-matter
  ├─ optional edf-http-diagnostics and edf-http-ota
  └─ edf-core ← shared domain, lifecycle, reset and outbound ports

host tools: edf-pairing-tool → edf-matter → edf-core
frontend: dashboard-core receives device JSX, status and callbacks
```

`edf-core` has no concrete hardware platform, network stack or Matter crate dependency. Its permitted Embassy watch/timer and logging dependencies support lifecycle and reset orchestration; generic embedded HAL traits may appear transitively through Embassy. `edf-gpio` provides constructor-supplied ESP flash, button and LED adapters; board pin numbers are consumer choices. `edf-ota` verifies the standard Ed25519 signature over the raw firmware image before committing an inactive A/B slot. The shared OTA wire header is `ICOT` plus a little-endian length and 64-byte signature. The 2026-10-22 user-approved safety guard rejects exhausted, reserved, non-increasing or wrong-slot boot sequences before `otadata` writes; it retains the ESP bootloader's numeric sequence ordering and valid fallback. The extraction and guard are commits `8bfb393` and `dacb7bb` respectively.

After an OTA image is verified, `FlashOtaDriver` latches a reboot-required state before attempting its boot metadata commit. The same driver rejects further uploads before reading a stream or touching image flash while that state is set. A commit failure may have partially written `otadata`, so it also requires reboot before another attempt. Truncation, image flash, and signature failures before the commit boundary leave the driver retryable against the inactive slot. Reloading bootloader metadata does not clear the latch; a new driver is created after reboot.

The two HTTP crates are independent, optional features. They accept parsed routes, byte streams, borrowed assets and consumer diagnostics, then return response data and deferred effects. They own neither TCP framing nor a listener. The caller must write and flush a successful response before invoking an action finalizer that can erase credentials or reboot. Device-specific JSON fields are supplied through `DiagnosticPayload`; device-specific routes and controls remain in the consumer.

`edf-matter` implements fixed-buffer device identity, a Wi-Fi/Matter partitioned key store, consumer-selected readiness bits, fabric policy and connection monitoring. Matter endpoint declarations and `rs-matter-embassy` runtime wrappers are consumer-owned. The host CLI uses this identity code to produce commissioning values with supplied serial/hostname prefixes. `packages/dashboard-core` exposes a presentational Solid shell, common tabs/components and a typed API with injected fetch; consumers own device state, controls, polling, CSS inclusion and transport origin. The package treats `solid-js` as a peer and externalizes it from its library output.

The Rust workspace uses exact aligned direct versions and committed `Cargo.lock`; the dashboard has exact frontend versions and committed `bun.lock`. Rust 1.99.1 and Bun 1.3.14 match the 2026-10-21 aligned HVAC and Intercom baselines. The only external source path is the root `rs-matter` patch at `../rs-matter/rs-matter`, expected at compatibility commit `d1fa97f`. Cargo does **not** record that local checkout revision, so reproducing results requires checking it separately. The adjacent `rs-matter-stack` (`2b8b21e`) and `rs-matter-embassy` (`18e05b5`) checkouts remain consumer dependencies, not foundation dependencies. Their source was not changed by this build.

This repository does not contain a consumer submodule, full device firmware, a network server, physical OTA validation, or a hardware deployment. The next phase can integrate each consumer separately, preserve its existing behavior and run complete firmware and board checks before retiring duplicate adapters.

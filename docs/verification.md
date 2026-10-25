# Verification record

This record covers the standalone `codex/foundation-build` branch from baseline `bb5ec3b` through the reviewed OTA retry fix `2f91869`. Initial Task 9 Rust gates ran against `c63743f`; all listed gates were repeated successfully after the final runtime fix at `2f91869`. Later handoff documentation changes do not change production code, manifests or locks. All Rust commands use `mise`; host checks target `x86_64-apple-darwin`, and device library checks target `riscv32imac-unknown-none-elf`.

## Completed task gates

| Task | Commit range | Verified evidence |
| --- | --- | --- |
| 1 core | `bb5ec3b..8d5166b` | 19 host tests; device library check; strict host Clippy |
| 2 logger | `8d5166b..49b0a42` | 8 host tests; device library check; strict host Clippy |
| 3 GPIO | `49b0a42..39412da` | 14 host tests; device library check; strict host Clippy and format |
| 4 OTA | `39412da..dacb7bb` | 56 host tests; device library check; strict host Clippy and format; approved sequence safety regression in `dacb7bb` |
| 5 HTTP | `dacb7bb..124662c` | 17 host integration tests; combined and independent HTTP device checks; strict host Clippy and format; post-flush effects tested in `124662c` |
| 6 Matter | `124662c..cb5e67c` | 15 host tests; device library and core dependency checks; strict host Clippy and format |
| 7 host tools | `cb5e67c..3ad8303` | 11 Rust tests, 19 Python unittests, exact HVAC/Intercom JSON goldens, strict Clippy/format, three script help commands |
| 7a test colocation | `3ad8303..0210750` | All 140 Rust tests preserved: 104 inline and 36 external, 389 assertion occurrences; strict Clippy, seven device library checks and format |
| 8 dashboard | `0210750..c63743f` | Frozen offline Bun install, source/fixture typechecks, Biome, 12 Bun tests with 34 assertions, library/declarations and single-file fixture build, CSS/resource audit and desktop browser review |
| Final OTA retry safety | `15474b4..2f91869` | 144 host Rust tests, strict Clippy, rustdoc, format, seven MCU libraries, independent HTTP checks and dependency trees; real-driver/HTTP failed-flush retry regression |

The Task 4 approved safety deviation retains ordinary OTA bytes, Ed25519 verification and ESP bootloader numeric selection while rejecting invalid or exhausted sequence writes before `otadata` mutation. No rollover rewrite or fallback erase was implemented. The Task 7a commit only relocates 56 tests into their source modules; production APIs and behavior are unchanged. Task 8's follow-up commit `c63743f` fixes read-only/pending reset presentation and heap bounds. The final Rust runtime fix is `2f91869`; Python remains unchanged since `3ad8303` and Bun gates remain applicable at unchanged dashboard revision `c63743f`.

## Final Task 9 gates

The following fresh checks all exited 0 on both `c63743f` and the final runtime revision `2f91869`:

```sh
CARGO_INCREMENTAL=0 mise exec -- cargo test --workspace --target x86_64-apple-darwin --locked --offline --quiet
CARGO_INCREMENTAL=0 mise exec -- cargo clippy --workspace --all-targets --target x86_64-apple-darwin --locked --offline -- -D warnings
RUSTDOCFLAGS='-D warnings' CARGO_INCREMENTAL=0 mise exec -- cargo doc --workspace --no-deps --target x86_64-apple-darwin --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo check -p edf-core -p edf-logger -p edf-gpio -p edf-ota -p edf-http-diagnostics -p edf-http-ota -p edf-matter --lib --target riscv32imac-unknown-none-elf --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo check -p edf-http-diagnostics --lib --target riscv32imac-unknown-none-elf --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo check -p edf-http-ota --lib --target riscv32imac-unknown-none-elf --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo tree -p edf-core --edges normal --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo tree -p edf-http-diagnostics -p edf-http-ota --edges normal --depth 1 --locked --offline
mise exec -- cargo fmt --all --check
```

The initial workspace run passed 140 Rust tests; the final run passed all 144 (104 inline unit tests and 40 external integration/CLI tests). Clippy, rustdoc with `-D warnings`, format, the combined seven-library MCU check and both separate HTTP MCU checks passed. The core's normal direct dependencies are `embassy-sync 0.8.0`, `embassy-time 0.5.1` and `log 0.4.34`; no concrete ESP platform or Matter crate is in its normal dependency tree. Generic embedded HAL traits may appear transitively through Embassy. Each HTTP feature's depth-one normal tree contains only `edf-core` and `embassy-time`; neither HTTP crate requires the other. The prior unused `rs-matter` patch warning was absent. The MCU check did print `esp-hal`'s standard advice to use a release profile; that advisory was not suppressed and the debug-profile library checks still exited 0. It does not establish physical timing or flash behavior.

## Audits and limits

The root Rust shared direct dependency versions were compared with aligned HVAC `61430d1` and Intercom `c22b52e`; extra foundation workspace entries are pinned versions from the approved specification. Crate-specific feature selections remain at adapter edges. The dashboard's 14 direct frontend versions match both aligned package manifests. All three `mise.toml` files pin Rust `1.99.1` and Bun `1.3.14`. `Cargo.lock` and dashboard `bun.lock` are tracked. Foundation crate manifests inherit workspace pins; the host pairing crate depends on the sibling foundation Matter crate. No foundation production module imports a consumer crate or requires an absolute user path; HVAC/Intercom names appear in test vectors and examples. A source search found no `todo!`, `unimplemented!`, TODO/FIXME or stub implementation in production code. The root path patch `../rs-matter/rs-matter` is the only external source checkout and is not Git-pinned by Cargo; its verified checkout revision was `d1fa97f` (adjacent consumer-only checkouts were `rs-matter-stack 2b8b21e` and `rs-matter-embassy 18e05b5`).

The host suites exercise mocks, cryptographic test vectors, route and stream behavior, transition policy and UI fixture states. The RISC-V checks compile seven libraries only. They do not validate complete consumer firmware, physical GPIO/flash, a Matter controller, a live HTTP server, serial hardware, a real signing key, an OTA upload, or bootloader rollback on a board. Consumer integration, submodule installation and full device validation remain future work.

The post-review OTA retry regression uses the real `FlashOtaDriver` with the HTTP OTA handler: a successful commit followed by failed response flush leaves the device running without reboot; another request is rejected without consuming its stream, erasing/writing either image slot, or changing `otadata`. Separate driver workflows confirm that signature and truncation failures before metadata commit still permit an inactive-slot retry, while a simulated partial metadata write error requires reboot. These host tests use in-memory flash and deterministic test keys; physical rollback and transport behavior remain unverified.

Independent whole-foundation review and focused re-review completed on 2026-10-23 with a **ready for standalone handoff** verdict at `2f91869`, with no remaining Critical or Important findings. Two nonblocking Minor notes remain: `DeviceIdentity` exposes public fixed buffers and lengths by design, so external mutation can violate accessor assumptions; and successful ELF conversion in `scripts/ota.py` leaves the generated temporary `.bin` file. `edf-logger::init` error propagation compiled and was inspected, but runtime registration was not tested because `log` permits one process-global logger. These are recorded as observed limits, not claimed passing behaviors.

## Neutral dashboard fixture follow-up (2026-10-23)

At the user's request, the preview's product-specific scenarios were replaced by synthetic Sample Alpha/Beta data. The renderer receives an open-ended, nonempty list of fixture scenarios; it does not branch on product identities. Presentation test data and the package usage example are neutral too. The exported dashboard implementation, public API, dependency versions, Rust and Python sources are unchanged.

The new boundary and sample-coverage tests failed against the previous fixture, then passed after the refactor. Final dashboard verification passed all 14 Bun tests (45 assertions), source and fixture typechecks, Biome, library JavaScript/declaration build, and the single-file fixture build. The latter emitted only Vite plugin-timing advice. Desktop browser checks covered both samples, per-sample theme restoration, true/false/absent subscription states, endpoint rows, null diagnostics, fake logs and read-only reset. The temporary loopback preview and browser tab were closed afterward. No hardware or live device endpoint was used.

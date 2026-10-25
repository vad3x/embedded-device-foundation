# Embedded Device Foundation

Shared `no_std` domain and adapter crates for embedded devices. Dependencies point inward: device firmware depends on adapters, adapters depend on `edf-core` ports and domain types, and `edf-core` has no concrete platform or Matter dependency.

Use `mise` for Rust and Bun. The workspace defaults to the host target; select the MCU target explicitly. Use `CARGO_INCREMENTAL=0` for Rust verification and `--locked --offline` when validating against the committed lock and local cache.

Place Rust unit tests in inline `#[cfg(test)] mod tests` beside the implementation they exercise. Reserve `tests/` for cross-component integration workflows and executable CLI tests. Keep test-only `std` imports under test configuration so device libraries remain `no_std`.

From the repository root, run the whole host suite with `CARGO_INCREMENTAL=0 mise exec -- cargo test --workspace --target x86_64-apple-darwin --locked --offline`. Run strict host lint with `CARGO_INCREMENTAL=0 mise exec -- cargo clippy --workspace --all-targets --target x86_64-apple-darwin --locked --offline -- -D warnings`. Run documentation with `RUSTDOCFLAGS='-D warnings' CARGO_INCREMENTAL=0 mise exec -- cargo doc --workspace --no-deps --target x86_64-apple-darwin --locked --offline`.

Check the seven device libraries with `CARGO_INCREMENTAL=0 mise exec -- cargo check -p edf-core -p edf-logger -p edf-gpio -p edf-ota -p edf-http-diagnostics -p edf-http-ota -p edf-matter --lib --target riscv32imac-unknown-none-elf --locked --offline`. `edf-pairing-tool` is a host CLI. For the dashboard, work from `packages/dashboard-core` and use the package scripts documented in its README. Python host-tool tests run with `python3 -m unittest discover -s scripts/tests -v` from the root.

The repository is a standalone foundation. Do not change consumer imports or duplicate adapters here. The workspace patch for `rs-matter` expects a sibling `../rs-matter/rs-matter` checkout at the aligned `d1fa97f` revision; the path dependency itself is not revision-pinned by Cargo. Host and RISC-V library checks do not validate physical hardware, OTA upload, or consumer integration. See `docs/architecture.md` and `docs/verification.md` for scope and evidence.

# Embedded Device Foundation

Reusable `no_std` domain and adapter libraries for ESP32-C6 devices, plus a host Matter pairing tool, safe host scripts, and a shared Solid.js dashboard package. The repository is currently **standalone**: HVAC Bridge and Intercom Matter remain on their aligned, working implementations. Consumer imports, submodules, firmware composition, and deployment belong to a later integration phase.

## What's here

| Area | Packages | Responsibility |
| --- | --- | --- |
| Domain and ports | `edf-core` | Lifecycle, reset, diagnostics, ecosystem records, OTA ports; no concrete platform or Matter dependency |
| Platform adapters | `edf-logger`, `edf-gpio`, `edf-ota` | Fixed-buffer logging, ESP flash/button/LED, signed A/B OTA |
| Optional HTTP features | `edf-http-diagnostics`, `edf-http-ota` | Route parsing, JSON, streamed OTA handling, deferred effects; no TCP listener |
| Matter and host tools | `edf-matter`, `edf-pairing-tool`, `scripts/` | Identity, storage, connection policy, commissioning codes, OTA/key/serial utilities |
| Dashboard | `packages/dashboard-core` | Solid shell, diagnostics, ecosystem and log views, typed common HTTP API |

Device-owned state, endpoint metadata, GPIO pin selection, HTTP transport, network runtime, dashboard controls and polling, and firmware startup remain in the consumers. [Architecture](docs/architecture.md) explains the boundaries.

## Setup and checks

Install the pinned Rust 1.99.1 and Bun 1.3.14 through `mise`. The Cargo workspace patch resolves `rs-matter` from the sibling path `../rs-matter/rs-matter`; use the aligned compatibility checkout at `d1fa97f`. This is a local path patch, so `Cargo.lock` does not pin that checkout's Git revision. Its location and revision are prerequisites for reproducing the Rust checks.

From the repository root:

```sh
CARGO_INCREMENTAL=0 mise exec -- cargo test --workspace --target x86_64-apple-darwin --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo clippy --workspace --all-targets --target x86_64-apple-darwin --locked --offline -- -D warnings
RUSTDOCFLAGS='-D warnings' CARGO_INCREMENTAL=0 mise exec -- cargo doc --workspace --no-deps --target x86_64-apple-darwin --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo check -p edf-core -p edf-logger -p edf-gpio -p edf-ota -p edf-http-diagnostics -p edf-http-ota -p edf-matter --lib --target riscv32imac-unknown-none-elf --locked --offline
python3 -m unittest discover -s scripts/tests -v
```

The seven RISC-V checks compile libraries; they do not build or exercise complete firmware on a board. `edf-pairing-tool` is a host executable. See [verification](docs/verification.md) for exact gate evidence and limits.

For the dashboard, run the frozen Bun install, checks, tests, and builds from `packages/dashboard-core` as shown in its [package README](packages/dashboard-core/README.md). The fixture produces one HTML file with inlined JS/CSS; consumers still provide their own styling setup and device controls.

## Host tools

The pairing CLI accepts explicit branding and a MAC address, or reads a connected board through `espflash`:

```sh
mise exec -- cargo qr --prefix HV --name hvac-bridge --mac 98:a3:16:a6:78:44 --json
mise exec -- cargo qr --prefix IC --name intercom-bridge --mac 98:a3:16:a6:78:44
```

`scripts/ota.py` requires a firmware path, `--device-type`, and `--key`; it checks the selected device's reported type before signing/uploading. `scripts/generate_keys.py --key PATH` creates a new private key without overwriting an existing one. `scripts/monitor.py` defaults to a non-resetting serial connection; `--reset` opts into DTR/RTS reset. Run each script with `--help` for the full interface. The scripts' optional Python dependencies are `cryptography` for signing/key generation and `zeroconf` for mDNS discovery; ELF conversion also invokes `espflash` through `mise`. No real key, board, OTA upload, or network endpoint was used for foundation verification.

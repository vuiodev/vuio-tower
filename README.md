# VuIO Tower

This is the Tauri-based desktop GUI for VuIO. It embeds the Rust backend as a sidecar process.

## Prerequisites

Ensure you have the following installed on your system:
- **Rust / Cargo** (via [rustup](https://rustup.rs/))
- **Node.js** (for web frontend development/Tauri CLI tools)

## How to Run Locally

### 1. Build the Backend Sidecar

The desktop app expects the backend `vuio` server binary to be placed in `src-tauri/binaries` with the system's target triple suffix.

From the repository root (where the backend is located), build the backend and copy it into the GUI sidecars directory:

```bash
# 1. Build the backend
cargo build --bin vuio

# 2. Copy the binary to the Tauri sidecar folder with the correct host target triple
cp target/debug/vuio gui/src-tauri/binaries/vuio-$(rustc -Vv | grep host | cut -d' ' -f2)
```

### 2. Run the Desktop App

You can run the Tauri application either using Cargo directly or through the Tauri CLI.

#### Option A: Run using Cargo (Directly)
From the `gui/src-tauri` directory:
```bash
cargo run
```

#### Option B: Run using Tauri CLI (Recommended for development)
First, ensure frontend dependencies are installed. From the `gui/` directory:
```bash
npm install
npm run tauri dev
```

---

## Capabilities & Configuration

- **Config Location**: On startup, the sidecar creates/uses a default configuration file at `~/Library/Application Support/vuio/config.toml` (macOS) or the standard Linux config path.
- **Port**: The default UI port is `8080`.
- **Sidecar Permissions**: Sidecar execution permissions are configured in `gui/src-tauri/capabilities/default.json`.

## Contributing

Contributions welcome! Please ensure cross-platform compatibility is maintained.
Input license = output license

## License

- [MIT License](LICENSE-MIT)
or
- [Apache License 2.0](LICENSE-APACHE)

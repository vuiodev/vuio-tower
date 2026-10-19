# VuIO Tower

VuIO Tower is the Tauri desktop host for `vuio-core`. The complete VuIO server runs inside the desktop process; there is no copied executable or sidecar process.

## Prerequisites

- Rust and Cargo
- Node.js for the Tauri CLI

The `vuio-core` dependency is resolved directly from the upstream `vuiodev/vuio` Git repository.

## Run locally

```bash
npm install
npm run tauri dev
```

To run the Rust host directly:

```bash
cargo run --manifest-path src-tauri/Cargo.toml
```

## Configuration

Tower uses the standard VuIO configuration directory and starts the embedded runtime automatically. The settings form patches the existing TOML document instead of regenerating it, preserving comments, unknown fields, and advanced directory options.

The default HTTP dashboard is available on port `8080`. Server lifecycle operations are performed through `vuio-core`; no Tauri shell execution permission is required.

## License

MIT or Apache-2.0.

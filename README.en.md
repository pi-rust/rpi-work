# RPI Work

> A native desktop Agent workbench built with Rust and GPUI. GPUI is the sole maintained UI entry point.

[中文 README](README.md) · [Latest release](https://github.com/pi-rust/rpi-work/releases/latest) · [All releases](https://github.com/pi-rust/rpi-work/releases) · [CI](https://github.com/pi-rust/rpi-work/actions/workflows/build.yml) · [Documentation](#documentation)

The latest historical release is **RPI Work 0.1.22**. Run the current GPUI workbench from source.

> This project is released under the [MIT License](LICENSE). Commercial plans, official services, and paid support are still being planned; the MIT grant itself permits commercial use. See the [legal and commercial status](docs/legal/licensing.md) for details.

## Launch GPUI

From the repository root:

```bash
task desktop
```

`task`, `task dev`, and `task dev:desktop` open the same native window. You can also launch directly with Cargo:

```bash
cargo run --manifest-path apps/work-gpui/Cargo.toml
```

The initial window contains a labeled, disconnected layout sample. To read projects, sessions, and messages from a running Work node:

```bash
task desktop -- --connect http://127.0.0.1:13031
```

Set `RPI_WORK_PASSWORD` in the environment or `.env.local` when the node requires authentication. GPUI supports plain-text prompts, streamed replies, and session recovery. Release packages are still in development. See the [GPUI build notes](apps/work-gpui/README.md).

## Architecture

- [`apps/work-gpui`](apps/work-gpui) is the native desktop UI in a standalone Rust workspace.
- `work-client` and `work-viewmodel` provide the shared protocol client, session projection, transcript presentation, and theme tokens.
- `work-node-runtime`, `work-node-service`, and `work-node-host` own data, Agent execution, and authenticated transport.
- `work-cli` provides the CLI and the headless node entry point.
- `design/reui-message-layout/` is the visual and interaction reference. See the [migration record](docs/design/confirmed-workbench-migration.md).

`apps/work-ui` (Dioxus) and `apps/workbench-ui` (React/ReUI) are deprecated and kept for reference. Existing desktop, Web, and Android packages belong to the Dioxus release.

## Development checks

Recommended checks:

```bash
cargo fmt --manifest-path apps/work-gpui/Cargo.toml -- --check
cargo test --manifest-path apps/work-gpui/Cargo.toml
cargo clippy --manifest-path apps/work-gpui/Cargo.toml --all-targets -- -D warnings
```

CLI examples:

```bash
cargo run --locked -p work-cli -- --help
cargo run --locked -p work-cli -- projects
cargo run --locked -p work-cli -- sessions
cargo run --locked -p work-cli -- watch SESSION_ID
```

See [`Taskfile.yml`](Taskfile.yml), the [local build guide](docs/guides/ci-builds.md), and [testing and artifacts](docs/guides/testing-artifacts.md) for more development workflows.

## Security and privacy

- Do not commit node passwords, model keys, Android signing material, or update-signing keys.
- Treat sharing and Web listeners as trusted-network features; use HTTPS and additional network controls for public deployments.
- Android authorization QR codes are short-lived, single-use credentials; do not publish screenshots of them.
- Discovery only supplies candidate addresses; the client still verifies node identity, protocol, and health. Discovery is not authorization.
- Third-party resources remain subject to their own upstream licenses; see the inventory in [`assets/vendor`](assets/vendor).

## Contributing

Issues, documentation improvements, and pull requests are welcome. Before submitting code, run formatting checks, relevant tests, and Clippy. For Web, Android, or UI changes, include the platforms you verified and any uncovered boundaries.

- [Development standards](docs/guides/development-standards.md)
- [Project structure](docs/guides/project-structure.md)
- [Architecture overview](docs/architecture/overview.md)
- [Node connection](docs/guides/node-connection.md)
- [Updates and releases](docs/guides/automatic-updates.md)

## Documentation

- **Product and architecture**: [Product scope](docs/design/product-design.md) · [Architecture overview](docs/architecture/overview.md) · [Transport architecture](docs/architecture/transport.md) · [Node and session ownership](docs/design/node-session-ownership.md)
- **Usage and connection**: [Projects, sessions, and messages](docs/guides/codex-interactions.md) · [Node connection and QR](docs/guides/node-connection.md) · [ACP backends](docs/guides/acp-backends.md) · [Theme design](docs/guides/theme-design.md)
- **Resources and extensions**: [Extension architecture](docs/architecture/extensions.md) · [Native resource development](docs/guides/native-resource-authoring.md) · [Component composition](docs/architecture/component-composition.md)
- **Development and validation**: [Multi-platform builds](docs/guides/ci-builds.md) · [Automatic updates](docs/guides/automatic-updates.md) · [Testing and artifacts](docs/guides/testing-artifacts.md) · [Archive](docs/archive/README.md)

## Historical release

[Version v0.1.22](https://github.com/pi-rust/rpi-work/releases/tag/v0.1.22) uses Dioxus. GPUI packaging and updates are pending.

# rcompose

> **Docker Compose for Windows WSL Containers (`wslc.exe`) in Rust.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/Rust-1.92%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-Windows%2011%20%7C%20WSL-lightgrey.svg)

Microsoft's native WSL container preview (`wslc.exe`) runs Linux containers directly on Windows without Docker Desktop or Podman machine. However, it does not currently provide a native Compose tool.

**`rcompose`** brings the full Docker Compose experience to WSL containers:
- **Rust Performance & Safety**: High-speed, single native executable with asynchronous I/O and zero-cost abstractions.
- **DAG Parallel Orchestrator**: Uses Directed Acyclic Graphs (`petgraph`) and Tokio to start independent services concurrently while strictly enforcing `depends_on`.
- **Config Drift Detection**: Computes canonical SHA-256 hashes stored in container labels (`com.docker.compose.config-hash`) to recreate only modified containers on `rcompose up`.
- **Resilient Engine**: Automatically handles Windows kernel and WSL preview concurrency errors (`ERROR_SHARING_VIOLATION`, `ERROR_ALREADY_EXISTS`) via smart exponential backoff retries.
- **Rich Terminal UX**: Spinners and progress indicators with `indicatif`, multiplexed color-coded logs per service, and graceful `Ctrl+C` shutdown.

---

## Architecture

`rcompose` is organized as a modular Cargo Workspace:

```
rcompose/
├── Cargo.toml                 # Root workspace
├── crates/
│   ├── rcompose-spec/         # Compose file parser, variable interpolation, env loading
│   ├── rcompose-engine/       # ContainerEngine trait & WslcEngine subprocess driver with retries
│   ├── rcompose-core/         # DAG dependency scheduler, Tokio parallel executor & drift detector
│   └── rcompose-cli/          # CLI interface (Clap v4), terminal UI & log multiplexer
└── examples/                  # Ready-to-run Compose examples
```

---

## Installation & Build

### Requirements
- **Windows 11** with WSL Container preview (`wslc.exe`) installed.
- **Rust 1.92+** with `cargo`.

### Compiling from Source

```powershell
# Clone the repository
git clone https://github.com/ricardoborges/rcompose.git
cd rcompose

# Build optimized release binary
cargo build --release

# The compiled binary is at:
# target/release/rcompose.exe
```

You can copy `target/release/rcompose.exe` to a folder in your `PATH` (such as `C:\Users\<user>\.cargo\bin` or `C:\Program Files\WSL`).

---

## Quick Start

Navigate to any directory with a `compose.yaml` or `docker-compose.yml`:

```powershell
# Start services in the background
rcompose up -d

# Check container status
rcompose ps

# Stream color-coded logs for all services
rcompose logs -f

# Execute an interactive command inside a running container
rcompose exec web sh

# Stop and remove containers and networks
rcompose down -v
```

---

## Inspecting the Configuration

Run `rcompose config` to inspect the resolved configuration with variables applied:

```powershell
rcompose config
rcompose config --format json
```

---

## Supported Commands

| Command | Description |
|---|---|
| `rcompose up [-d] [--build] [--force-recreate]` | Start services in DAG order (runs independent services concurrently) |
| `rcompose down [-v] [-t <secs>]` | Stop and remove containers and networks in reverse DAG order |
| `rcompose ps` | Show container status, ports, and health |
| `rcompose logs [-f] [-t] [-n <tail>] [services]` | Multiplexed colored streaming logs |
| `rcompose exec [-it] [-u <user>] <service> <cmd...>` | Execute command in running container |
| `rcompose start / stop / restart [services]` | Service lifecycle control |
| `rcompose build [--no-cache] [--pull] [services]` | Build images defined in `build:` sections |
| `rcompose config [--format json\|yaml]` | Validate and view interpolated project spec |
| `rcompose version` | Show version info |

---

## License

MIT © [Ricardo Borges](https://github.com/ricardoborges)

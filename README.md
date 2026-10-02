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

### Installing from Source

```powershell
git clone https://github.com/ricardoborges/rcompose.git
cd rcompose

# Builds in release mode and installs to %CARGO_HOME%\bin (by default %USERPROFILE%\.cargo\bin, which rustup puts on PATH)
cargo install --path crates/rcompose-cli
```

Run the same command again to update. If you set a custom `CARGO_HOME` whose `bin` folder is not on `PATH`, add `--root "$env:USERPROFILE\.cargo"` to install into the rustup folder instead.

### Installing a Release Binary

Put `rcompose.exe` in a per-user folder such as `%LOCALAPPDATA%\Programs\rcompose` and add that folder to your user `PATH`. No administrator rights are needed.

Do not install it into `C:\Program Files\WSL`: that folder belongs to the WSL installer, which may remove or overwrite foreign files on updates or repairs. `rcompose` does not need to sit next to `wslc.exe`; it locates it through `WSLC_BIN`, then `PATH`, then `C:\Program Files\WSL\wslc.exe`.

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

Global options: `-f <file>`, `-p <project>`, `--env-file <file>`, `--profile <name>` (repeatable).

| Command | Description |
|---|---|
| `rcompose up [-d] [--build \| --no-build] [--force-recreate] [--remove-orphans] [-t <secs>] [services]` | Build/pull missing images and start services, honoring `depends_on` conditions |
| `rcompose down [-v] [--remove-orphans] [-t <secs>]` | Stop and remove containers and networks (and named volumes with `-v`) |
| `rcompose ps [-a] [-q] [services]` | Show container status, health, and ports |
| `rcompose logs [-f] [-t] [-n <tail>] [services]` | Multiplexed colored logs |
| `rcompose exec [-T] [-d] [-u <user>] [-w <dir>] [-e K=V] [--index N] <service> <cmd...>` | Execute a command in a running container |
| `rcompose start / stop / restart [-t <secs>] [services]` | Service lifecycle control |
| `rcompose build [--no-cache] [--pull] [services]` | Build images defined in `build:` sections |
| `rcompose pull [services]` | Pull service images |
| `rcompose config [--format json\|yaml] [--services] [-q]` | Validate and view the resolved project |
| `rcompose version` | Show version info |

---

## Compose Compatibility

`rcompose` reads standard `compose.yaml` / `docker-compose.yml` files:

- Variable interpolation (`${VAR}`, `${VAR:-default}`, `${VAR:?error}`, `${VAR:+alt}`, nesting, `$$`) from the process environment and `.env`
- YAML anchors and merge keys (`<<: *base`), `x-*` extension fields
- Short and long syntax for `ports`, `volumes`, `env_file` (with `required: false`) and `depends_on`
- `depends_on` conditions: `service_started`, `service_healthy`, `service_completed_successfully`
- `healthcheck`, multiple `networks` with `aliases`, `external` networks and volumes, `profiles`
- `build` (context, dockerfile, args, target), `deploy.replicas`, `deploy.resources.limits`, GPU reservations
- Project name precedence: `-p` > `COMPOSE_PROJECT_NAME` > `name:` > directory name

Keys the wslc engine cannot honor are reported as warnings instead of being dropped silently:

- `restart` policies (wslc has no restart support yet)
- Bind mounts of Linux host paths such as `/var/run/docker.sock`: wslc only accepts Windows paths as bind sources, so the mount is skipped (otherwise wslc would create the path, e.g. `D:\var\run\docker.sock`, on the current drive)
- `privileged`, `cap_add`, `devices`, `extra_hosts`, `secrets`, `configs`, `network_mode`, among others

---

## License

MIT © [Ricardo Borges](https://github.com/ricardoborges)

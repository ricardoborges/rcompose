# rcompose

Docker Compose for Windows' new WSL containers (`wslc.exe`), written in Rust.

Website: https://ricardoborges.github.io/rcompose/

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/Rust-1.92%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-Windows%2011%20%7C%20WSL-lightgrey.svg)

Microsoft's WSL container preview lets you run Linux containers on Windows without Docker Desktop or a Podman machine. What it doesn't have is Compose. I wanted to keep using my existing `compose.yaml` files, so I wrote `rcompose`: you run `rcompose up` in a folder and it drives `wslc` for you.

A few things it does that you might care about:

- Services that don't depend on each other start in parallel; `depends_on` (including `service_healthy`) is still respected.
- `rcompose up` only recreates containers whose configuration actually changed. It stores a hash of each service's config in a container label, the same way Docker Compose does.
- The WSL preview sometimes fails with transient errors like `ERROR_SHARING_VIOLATION` when several containers start at once. `rcompose` retries those instead of giving up.
- `rcompose logs -f` interleaves every service's output with a color per service, and `Ctrl+C` shuts things down cleanly.

## Installing

You need Windows 11 with the WSL container preview (`wslc.exe`) installed.

The easiest way is the install script:

```powershell
irm https://ricardoborges.github.io/rcompose/install.ps1 | iex
```

It grabs the latest release for your machine (x64 or ARM64), puts `rcompose.exe` in `%LOCALAPPDATA%\Programs\rcompose` and adds that folder to your user `PATH`. No admin rights needed. Run it again whenever you want to update. If you want a specific version or a different folder, set `RCOMPOSE_VERSION` (e.g. `v0.1.0`) or `RCOMPOSE_INSTALL_DIR` first.

If you'd rather build it yourself, you'll need Rust 1.92 or newer:

```powershell
git clone https://github.com/ricardoborges/rcompose.git
cd rcompose
cargo install --path crates/rcompose-cli
```

That installs into `%USERPROFILE%\.cargo\bin`, which rustup already puts on your `PATH`. (If you use a custom `CARGO_HOME` that isn't on `PATH`, add `--root "$env:USERPROFILE\.cargo"`.)

One thing to avoid: don't drop `rcompose.exe` into `C:\Program Files\WSL`. That folder belongs to the WSL installer, which can remove files it doesn't recognize when it updates or repairs itself. `rcompose` doesn't need to live next to `wslc.exe`; it looks for it in `WSLC_BIN`, then on `PATH`, then at `C:\Program Files\WSL\wslc.exe`.

## Using it

If you've used `docker compose`, you already know how this works. From a folder with a `compose.yaml` or `docker-compose.yml`:

```powershell
rcompose up -d          # start everything in the background
rcompose ps             # see what's running
rcompose logs -f        # follow the logs
rcompose exec web sh    # open a shell in the "web" service
rcompose down -v        # stop and clean up, including named volumes
```

To see the final config after variables and `.env` are applied, run `rcompose config` (add `--format json` if you prefer JSON).

### Commands

Global options: `-f <file>`, `-p <project>`, `--env-file <file>`, `--profile <name>` (repeatable).

| Command | What it does |
|---|---|
| `rcompose up [-d] [--build \| --no-build] [--force-recreate] [--remove-orphans] [-t <secs>] [services]` | Build or pull missing images and start services, waiting on `depends_on` conditions |
| `rcompose down [-v] [--remove-orphans] [-t <secs>]` | Stop and remove containers and networks (and named volumes with `-v`) |
| `rcompose ps [-a] [-q] [services]` | Container status, health and ports |
| `rcompose logs [-f] [-t] [-n <tail>] [services]` | Logs from all services, color-coded |
| `rcompose exec [-T] [-d] [-u <user>] [-w <dir>] [-e K=V] [--index N] <service> <cmd...>` | Run a command in a running container |
| `rcompose start / stop / restart [-t <secs>] [services]` | Start, stop or restart services |
| `rcompose build [--no-cache] [--pull] [services]` | Build images from `build:` sections |
| `rcompose pull [services]` | Pull service images |
| `rcompose config [--format json\|yaml] [--services] [-q]` | Validate and print the resolved project |
| `rcompose version` | Print the version |

## What works and what doesn't

Most everyday Compose files should just work. Supported:

- Variable interpolation (`${VAR}`, `${VAR:-default}`, `${VAR:?error}`, `${VAR:+alt}`, nesting, `$$`) from your environment and `.env`
- YAML anchors and merge keys (`<<: *base`), `x-*` extension fields
- Short and long syntax for `ports`, `volumes`, `env_file` (including `required: false`) and `depends_on`
- `depends_on` conditions: `service_started`, `service_healthy`, `service_completed_successfully`
- `healthcheck`, multiple `networks` with `aliases`, `external` networks and volumes, `profiles`
- `build` (context, dockerfile, args, target), `deploy.replicas`, `deploy.resources.limits`, GPU reservations
- Project name from `-p`, then `COMPOSE_PROJECT_NAME`, then `name:`, then the folder name

Some things `wslc` simply can't do yet. When your file uses them, `rcompose` prints a warning rather than silently ignoring them:

- `restart` policies (wslc doesn't support restarts yet)
- Bind mounts from Linux host paths like `/var/run/docker.sock`. wslc only accepts Windows paths as bind sources, and passing it a Linux path would make it create something like `D:\var\run\docker.sock` on your current drive, so `rcompose` skips the mount.
- `privileged`, `cap_add`, `devices`, `extra_hosts`, `secrets`, `configs`, `network_mode` and a few others

## How the code is laid out

It's a Cargo workspace with four crates:

- `crates/rcompose-spec` parses Compose files, handles interpolation and `.env` loading
- `crates/rcompose-engine` talks to `wslc.exe` (and does the retrying)
- `crates/rcompose-core` works out the start order and runs services in parallel, plus the change detection
- `crates/rcompose-cli` is the command line and terminal output

There are some ready-to-run Compose files in [examples/](examples/).

## License

MIT © [Ricardo Borges](https://github.com/ricardoborges)

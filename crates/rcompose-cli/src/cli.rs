//! Command-line argument definitions using Clap v4.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "rcompose",
    author = "Ricardo Borges",
    version,
    about = "Docker Compose for Windows WSL Containers (wslc) in Rust",
    long_about = "A Docker Compose compatible tool that runs Compose projects on WSL containers (wslc)."
)]
pub struct Cli {
    #[arg(short = 'f', long = "file", global = true, help = "Path to compose file")]
    pub file: Option<PathBuf>,

    #[arg(short = 'p', long = "project-name", global = true, help = "Project name override")]
    pub project_name: Option<String>,

    #[arg(long = "env-file", global = true, help = "Path to an alternate environment file")]
    pub env_file: Option<PathBuf>,

    #[arg(long = "profile", global = true, help = "Enable a profile (can be repeated)")]
    pub profiles: Vec<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Create and start containers")]
    Up(UpArgs),

    #[command(about = "Stop and remove containers, networks, and volumes")]
    Down(DownArgs),

    #[command(about = "List containers")]
    Ps(PsArgs),

    #[command(about = "View output from containers")]
    Logs(LogsArgs),

    #[command(about = "Build or rebuild services")]
    Build(BuildArgs),

    #[command(about = "Pull service images")]
    Pull(ServiceListArgs),

    #[command(about = "Start services")]
    Start(ServiceListArgs),

    #[command(about = "Stop services")]
    Stop(StopArgs),

    #[command(about = "Restart service containers")]
    Restart(StopArgs),

    #[command(about = "Execute a command in a running container")]
    Exec(ExecArgs),

    #[command(about = "Validate and view the Compose file")]
    Config(ConfigArgs),

    #[command(about = "Show version information")]
    Version,
}

#[derive(Args, Debug)]
pub struct UpArgs {
    #[arg(short = 'd', long = "detach", help = "Detached mode: Run containers in the background")]
    pub detach: bool,

    #[arg(long = "build", conflicts_with = "no_build", help = "Build images before starting containers")]
    pub build: bool,

    #[arg(long = "no-build", help = "Don't build an image, even if it's missing")]
    pub no_build: bool,

    #[arg(long = "force-recreate", help = "Recreate containers even if their configuration has not changed")]
    pub force_recreate: bool,

    #[arg(long = "remove-orphans", help = "Remove containers for services not defined in the Compose file")]
    pub remove_orphans: bool,

    #[arg(short = 't', long = "timeout", help = "Shutdown timeout in seconds when containers are stopped")]
    pub timeout: Option<u32>,

    #[arg(help = "Services to start (defaults to all)")]
    pub services: Vec<String>,
}

#[derive(Args, Debug)]
pub struct DownArgs {
    #[arg(short = 'v', long = "volumes", help = "Remove named volumes declared in the volumes section")]
    pub volumes: bool,

    #[arg(long = "remove-orphans", help = "Remove containers for services not defined in the Compose file")]
    pub remove_orphans: bool,

    #[arg(short = 't', long = "timeout", help = "Specify a shutdown timeout in seconds")]
    pub timeout: Option<u32>,
}

#[derive(Args, Debug)]
pub struct PsArgs {
    #[arg(short = 'a', long = "all", help = "Show all containers, including stopped ones")]
    pub all: bool,

    #[arg(short = 'q', long = "quiet", help = "Only display container IDs")]
    pub quiet: bool,

    #[arg(help = "Filter by service names")]
    pub services: Vec<String>,
}

#[derive(Args, Debug)]
pub struct LogsArgs {
    #[arg(short = 'f', long = "follow", help = "Follow log output")]
    pub follow: bool,

    #[arg(short = 't', long = "timestamps", help = "Show timestamps")]
    pub timestamps: bool,

    #[arg(short = 'n', long = "tail", help = "Number of lines to show from the end of the logs")]
    pub tail: Option<String>,

    #[arg(help = "Services to show logs for")]
    pub services: Vec<String>,
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    #[arg(long = "no-cache", help = "Do not use cache when building the image")]
    pub no_cache: bool,

    #[arg(long = "pull", help = "Always attempt to pull a newer version of the image")]
    pub pull: bool,

    #[arg(help = "Services to build")]
    pub services: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ServiceListArgs {
    #[arg(help = "Target services")]
    pub services: Vec<String>,
}

#[derive(Args, Debug)]
pub struct StopArgs {
    #[arg(short = 't', long = "timeout", help = "Specify a shutdown timeout in seconds")]
    pub timeout: Option<u32>,

    #[arg(help = "Services to stop")]
    pub services: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ExecArgs {
    #[arg(short = 'T', long = "no-TTY", help = "Disable pseudo-TTY allocation")]
    pub no_tty: bool,

    #[arg(short = 'd', long = "detach", help = "Run command in the background")]
    pub detach: bool,

    #[arg(short = 'u', long = "user", help = "Run as specified username or uid")]
    pub user: Option<String>,

    #[arg(short = 'w', long = "workdir", help = "Path to workdir directory for this command")]
    pub workdir: Option<String>,

    #[arg(short = 'e', long = "env", help = "Set environment variables (KEY=VALUE)")]
    pub env: Vec<String>,

    #[arg(long = "index", default_value_t = 1, help = "Index of the container if the service has multiple replicas")]
    pub index: usize,

    #[arg(help = "Target service")]
    pub service: String,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true, help = "Command and arguments to execute")]
    pub command: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[arg(long = "format", default_value = "yaml", help = "Output format (yaml or json)")]
    pub format: String,

    #[arg(short = 'q', long = "quiet", help = "Only validate the configuration, do not print")]
    pub quiet: bool,

    #[arg(long = "services", help = "Print the service names, one per line")]
    pub services: bool,
}

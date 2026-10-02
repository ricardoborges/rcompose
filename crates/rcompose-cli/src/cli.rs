//! Command-line argument definitions using Clap v4.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "rcompose",
    author = "Ricardo Borges",
    version = "0.1.0",
    about = "Docker Compose for Windows WSL Containers (wslc) in Rust",
    long_about = "A high-performance, DAG-scheduled, WSL-native Docker Compose alternative built in Rust."
)]
pub struct Cli {
    #[arg(short = 'f', long = "file", help = "Path to compose file")]
    pub file: Option<PathBuf>,

    #[arg(short = 'p', long = "project-name", help = "Project name override")]
    pub project_name: Option<String>,

    #[arg(long = "env-file", help = "Path to an alternate environment file")]
    pub env_file: Option<PathBuf>,

    #[arg(long = "rcompose-file", help = "Path to rcompose.yml extension file")]
    pub rcompose_file: Option<PathBuf>,

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

    #[arg(long = "build", help = "Build images before starting containers")]
    pub build: bool,

    #[arg(long = "force-recreate", help = "Recreate containers even if their configuration has not changed")]
    pub force_recreate: bool,

    #[arg(long = "remove-orphans", help = "Remove containers for services not defined in the Compose file")]
    pub remove_orphans: bool,

    #[arg(help = "Services to start (defaults to all)")]
    pub services: Vec<String>,
}

#[derive(Args, Debug)]
pub struct DownArgs {
    #[arg(short = 'v', long = "volumes", help = "Remove named volumes declared in the volumes section")]
    pub volumes: bool,

    #[arg(long = "remove-orphans", help = "Remove containers for services not defined in the Compose file")]
    pub remove_orphans: bool,

    #[arg(short = 't', long = "timeout", default_value_t = 10, help = "Specify a shutdown timeout in seconds")]
    pub timeout: u32,
}

#[derive(Args, Debug)]
pub struct PsArgs {
    #[arg(short = 'a', long = "all", help = "Show all stopped containers as well")]
    pub all: bool,

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
    pub tail: Option<usize>,

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
    #[arg(short = 't', long = "timeout", default_value_t = 10, help = "Specify a shutdown timeout in seconds")]
    pub timeout: u32,

    #[arg(help = "Services to stop")]
    pub services: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ExecArgs {
    #[arg(short = 'i', long = "interactive", default_value_t = true, help = "Keep STDIN open even if not attached")]
    pub interactive: bool,

    #[arg(short = 't', long = "tty", default_value_t = true, help = "Allocate a pseudo-TTY")]
    pub tty: bool,

    #[arg(short = 'u', long = "user", help = "Run as specified username or uid")]
    pub user: Option<String>,

    #[arg(short = 'w', long = "workdir", help = "Path to workdir directory for this run")]
    pub workdir: Option<String>,

    #[arg(help = "Target service")]
    pub service: String,

    #[arg(trailing_var_arg = true, required = true, help = "Command and arguments to execute")]
    pub command: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[arg(long = "format", default_value = "yaml", help = "Output format (yaml or json)")]
    pub format: String,

    #[arg(short = 'q', long = "quiet", help = "Only validate the configuration, do not print")]
    pub quiet: bool,
}

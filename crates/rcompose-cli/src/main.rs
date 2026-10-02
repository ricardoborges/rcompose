//! rcompose: Docker Compose for Windows WSL Containers (wslc) in Rust.

mod cli;
mod commands;
mod logs;
mod ui;

use clap::Parser;
use cli::{Cli, Commands};
use commands::{config, down, exec, lifecycle, logs as logs_cmd, ps, up};
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::WslcEngine;
use rcompose_spec::loader::{find_compose_file, load_project, LoadOptions};
use std::env;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{} {:#}", colored::Colorize::bold(colored::Colorize::red("Error:")), err);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let args = Cli::parse();

    if matches!(args.command, Commands::Version) {
        println!("rcompose version {} (wslc engine)", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let current_dir = env::current_dir()?;
    let compose_path = match args.file {
        Some(ref p) if !p.is_file() => anyhow::bail!("Compose file not found at: {}", p.display()),
        Some(ref p) => p.clone(),
        None => find_compose_file(&current_dir).ok_or_else(|| {
            anyhow::anyhow!(
                "no compose file found (looked for compose.yaml / compose.yml / docker-compose.yaml / docker-compose.yml in '{}' and parents)",
                current_dir.display()
            )
        })?,
    };

    let opts = LoadOptions {
        project_name: args.project_name.clone(),
        env_file: args.env_file.clone(),
        profiles: args.profiles.clone(),
        ..Default::default()
    };
    let project = load_project(&compose_path, opts).map_err(|e| anyhow::anyhow!("failed to load project: {}", e))?;
    for warning in &project.warnings {
        ui::warn(warning);
    }

    // `config` doesn't require the wslc engine
    if let Commands::Config(config_args) = args.command {
        return config::handle_config(&project, config_args);
    }

    let engine = WslcEngine::new().map_err(|e| anyhow::anyhow!("failed to initialize the wslc engine: {}", e))?;
    let orchestrator = Orchestrator::new(project, engine).with_reporter(ui::progress_reporter());

    match args.command {
        Commands::Up(a) => up::handle_up(&orchestrator, a).await,
        Commands::Down(a) => down::handle_down(&orchestrator, a).await,
        Commands::Ps(a) => ps::handle_ps(&orchestrator, a).await,
        Commands::Logs(a) => logs_cmd::handle_logs(&orchestrator, a).await,
        Commands::Exec(a) => exec::handle_exec(&orchestrator, a),
        Commands::Start(a) => lifecycle::handle_start(&orchestrator, a).await,
        Commands::Stop(a) => lifecycle::handle_stop(&orchestrator, a).await,
        Commands::Restart(a) => lifecycle::handle_restart(&orchestrator, a).await,
        Commands::Build(a) => lifecycle::handle_build(&orchestrator, a).await,
        Commands::Pull(a) => lifecycle::handle_pull(&orchestrator, a).await,
        Commands::Config(_) | Commands::Version => unreachable!(),
    }
}

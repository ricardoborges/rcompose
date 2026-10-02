//! rcompose: Docker Compose for Windows WSL Containers (wslc) in Rust.

mod cli;
mod commands;
mod logs;
mod signals;
mod ui;

use clap::Parser;
use cli::{Cli, Commands};
use commands::{config, down, exec, lifecycle, logs as logs_cmd, ps, up};
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::WslcEngine;
use rcompose_spec::loader::{find_compose_file, load_project, LoadOptions};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    if matches!(args.command, Commands::Version) {
        println!("rcompose version 0.1.0 (wslc engine)");
        return Ok(());
    }

    // 1. Locate compose file
    let current_dir = env::current_dir()?;
    let compose_path = if let Some(ref p) = args.file {
        if !p.is_file() {
            anyhow::bail!("Compose file not found at: {}", p.display());
        }
        p.clone()
    } else if let Some(p) = find_compose_file(&current_dir) {
        p
    } else {
        anyhow::bail!(
            "No compose file found (looked for compose.yaml / compose.yml / docker-compose.yml in '{}' and parents)",
            current_dir.display()
        );
    };

    // 2. Locate rcompose.yml extension if present
    let rcompose_path = args.rcompose_file.as_ref().map(|p| p.as_path());

    // 3. Load Project spec
    let opts = LoadOptions {
        project_name: args.project_name.clone(),
        env_file: args.env_file.clone(),
        ..Default::default()
    };

    let project = load_project(&compose_path, rcompose_path, opts)
        .map_err(|e| anyhow::anyhow!("Failed to load project: {}", e))?;

    // Handle Config command early (doesn't require wslc.exe engine)
    if let Commands::Config(config_args) = args.command {
        return config::handle_config(&project, config_args);
    }

    // 4. Initialize WSLC Engine and Orchestrator
    let engine = WslcEngine::new()
        .map_err(|e| anyhow::anyhow!("Failed to initialize WSLC engine: {}", e))?;
    let orchestrator = Orchestrator::new(project, engine);

    // 5. Setup signal handler
    let _running = signals::setup_ctrl_c();

    // 6. Execute Subcommands
    match args.command {
        Commands::Up(up_args) => up::handle_up(&orchestrator, up_args).await?,
        Commands::Down(down_args) => down::handle_down(&orchestrator, down_args).await?,
        Commands::Ps(ps_args) => ps::handle_ps(&orchestrator, ps_args).await?,
        Commands::Logs(logs_args) => logs_cmd::handle_logs(&orchestrator, logs_args).await?,
        Commands::Exec(exec_args) => exec::handle_exec(&orchestrator, exec_args)?,
        Commands::Start(start_args) => lifecycle::handle_start(&orchestrator, start_args).await?,
        Commands::Stop(stop_args) => lifecycle::handle_stop(&orchestrator, stop_args).await?,
        Commands::Restart(restart_args) => lifecycle::handle_restart(&orchestrator, restart_args).await?,
        Commands::Build(build_args) => lifecycle::handle_build(&orchestrator, build_args).await?,
        Commands::Config(_) | Commands::Version => unreachable!(),
    }

    Ok(())
}

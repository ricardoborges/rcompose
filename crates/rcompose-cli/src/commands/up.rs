use crate::cli::UpArgs;
use crate::logs::{stream_logs, LogOptions};
use colored::Colorize;
use rcompose_core::orchestrator::{Orchestrator, UpOptions};
use rcompose_engine::WslcEngine;

pub async fn handle_up(orchestrator: &Orchestrator<WslcEngine>, args: UpArgs) -> anyhow::Result<()> {
    let services = (!args.services.is_empty()).then_some(args.services);

    let up_opts = UpOptions {
        services: services.clone(),
        force_recreate: args.force_recreate,
        build: args.build,
        no_build: args.no_build,
        remove_orphans: args.remove_orphans,
        timeout: args.timeout,
    };
    let events = orchestrator.up(up_opts).await?;

    if args.detach || events.is_empty() {
        return Ok(());
    }

    let containers: Vec<String> = events.iter().map(|ev| ev.container_name.clone()).collect();
    eprintln!("{}", "Attaching to logs (press Ctrl+C to stop the containers)...".dimmed());
    let log_opts = LogOptions { follow: true, timestamps: false, tail: None };

    tokio::select! {
        res = stream_logs(orchestrator.engine().binary().to_path_buf(), &containers, log_opts) => res?,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("{}", "Gracefully stopping... (press Ctrl+C again to force)".yellow());
            tokio::spawn(async {
                if tokio::signal::ctrl_c().await.is_ok() {
                    std::process::exit(130);
                }
            });
            let names: Option<Vec<String>> = services;
            orchestrator.stop(names.as_deref(), args.timeout).await?;
        }
    }
    Ok(())
}

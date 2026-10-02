use crate::cli::LogsArgs;
use crate::logs::{stream_logs, LogOptions};
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::WslcEngine;

pub async fn handle_logs(orchestrator: &Orchestrator<WslcEngine>, args: LogsArgs) -> anyhow::Result<()> {
    let containers: Vec<String> = orchestrator
        .ps()
        .await?
        .into_iter()
        .filter(|c| args.services.is_empty() || args.services.contains(&c.service))
        .map(|c| c.name)
        .collect();

    if containers.is_empty() {
        eprintln!("No matching containers to show logs for.");
        return Ok(());
    }

    let opts = LogOptions { follow: args.follow, timestamps: args.timestamps, tail: args.tail };
    tokio::select! {
        res = stream_logs(orchestrator.engine().binary().to_path_buf(), &containers, opts) => res,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}

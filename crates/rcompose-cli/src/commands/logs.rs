use crate::cli::LogsArgs;
use crate::logs::LogMultiplexer;
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::wslc::discovery::find_wslc;
use rcompose_engine::WslcEngine;

pub async fn handle_logs(orchestrator: &Orchestrator<WslcEngine>, args: LogsArgs) -> anyhow::Result<()> {
    let containers = orchestrator.ps().await?;
    let wslc_bin = find_wslc().map_err(|e| anyhow::anyhow!(e))?;

    let filter_services = !args.services.is_empty();
    let targets: Vec<(String, String)> = containers
        .into_iter()
        .filter(|c| !filter_services || args.services.contains(&c.service))
        .map(|c| (c.name, c.service))
        .collect();

    if targets.is_empty() {
        println!("No matching containers to log.");
        return Ok(());
    }

    let multiplexer = LogMultiplexer::new(wslc_bin);
    multiplexer.stream_logs(&targets, args.follow, args.tail).await?;

    Ok(())
}

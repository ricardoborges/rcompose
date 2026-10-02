use crate::cli::{BuildArgs, ServiceListArgs, StopArgs};
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::WslcEngine;

fn targets(services: &[String]) -> Option<&[String]> {
    (!services.is_empty()).then_some(services)
}

pub async fn handle_start(orchestrator: &Orchestrator<WslcEngine>, args: ServiceListArgs) -> anyhow::Result<()> {
    orchestrator.start(targets(&args.services)).await?;
    Ok(())
}

pub async fn handle_stop(orchestrator: &Orchestrator<WslcEngine>, args: StopArgs) -> anyhow::Result<()> {
    orchestrator.stop(targets(&args.services), args.timeout).await?;
    Ok(())
}

pub async fn handle_restart(orchestrator: &Orchestrator<WslcEngine>, args: StopArgs) -> anyhow::Result<()> {
    orchestrator.restart(targets(&args.services), args.timeout).await?;
    Ok(())
}

pub async fn handle_build(orchestrator: &Orchestrator<WslcEngine>, args: BuildArgs) -> anyhow::Result<()> {
    orchestrator.build(targets(&args.services), args.pull, args.no_cache).await?;
    Ok(())
}

pub async fn handle_pull(orchestrator: &Orchestrator<WslcEngine>, args: ServiceListArgs) -> anyhow::Result<()> {
    orchestrator.pull(targets(&args.services)).await?;
    Ok(())
}

use crate::cli::DownArgs;
use rcompose_core::orchestrator::{DownOptions, Orchestrator};
use rcompose_engine::WslcEngine;

pub async fn handle_down(orchestrator: &Orchestrator<WslcEngine>, args: DownArgs) -> anyhow::Result<()> {
    let down_opts = DownOptions {
        remove_volumes: args.volumes,
        remove_orphans: args.remove_orphans,
        timeout: args.timeout,
    };
    orchestrator.down(down_opts).await?;
    Ok(())
}

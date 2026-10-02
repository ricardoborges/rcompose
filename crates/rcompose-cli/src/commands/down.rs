use crate::cli::DownArgs;
use crate::ui;
use colored::Colorize;
use rcompose_core::orchestrator::{DownOptions, Orchestrator};
use rcompose_engine::WslcEngine;

pub async fn handle_down(orchestrator: &Orchestrator<WslcEngine>, args: DownArgs) -> anyhow::Result<()> {
    let pb = ui::spinner("Stopping and removing containers...");

    let down_opts = DownOptions {
        remove_volumes: args.volumes,
        remove_orphans: args.remove_orphans,
        timeout_secs: args.timeout,
    };

    orchestrator.down(down_opts).await?;
    pb.finish_and_clear();

    ui::success(&format!(
        "Project {} stopped and removed",
        orchestrator.project().name.bold()
    ));

    Ok(())
}

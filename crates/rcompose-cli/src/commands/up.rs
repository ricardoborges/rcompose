use crate::cli::UpArgs;
use crate::logs::LogMultiplexer;
use crate::ui;
use colored::Colorize;
use rcompose_core::drift::DesiredAction;
use rcompose_core::orchestrator::{Orchestrator, UpOptions};
use rcompose_engine::wslc::discovery::find_wslc;
use rcompose_engine::WslcEngine;

pub async fn handle_up(orchestrator: &Orchestrator<WslcEngine>, args: UpArgs) -> anyhow::Result<()> {
    let pb = ui::spinner("Starting containers...");

    let services = if args.services.is_empty() {
        None
    } else {
        Some(args.services)
    };

    let up_opts = UpOptions {
        services,
        force_recreate: args.force_recreate,
        no_build: !args.build,
        remove_orphans: args.remove_orphans,
    };

    let events = orchestrator.up(up_opts).await?;
    pb.finish_and_clear();

    for ev in &events {
        match ev.action {
            DesiredAction::Create => {
                ui::success(&format!("Container {} Created", ev.container_name.bold()));
            }
            DesiredAction::Start => {
                ui::success(&format!("Container {} Started", ev.container_name.bold()));
            }
            DesiredAction::Recreate => {
                ui::success(&format!("Container {} Recreated", ev.container_name.bold()));
            }
            DesiredAction::UpToDate => {
                ui::info(&format!("Container {} Up to date", ev.container_name));
            }
        }
    }

    if !args.detach {
        let wslc_bin = find_wslc().map_err(|e| anyhow::anyhow!(e))?;
        let multiplexer = LogMultiplexer::new(wslc_bin);

        let targets: Vec<(String, String)> = events
            .iter()
            .map(|ev| (ev.container_name.clone(), ev.service.clone()))
            .collect();

        if !targets.is_empty() {
            println!("{}", "Attaching to logs (Press Ctrl+C to exit)...".dimmed());
            multiplexer.stream_logs(&targets, true, None).await?;
        }
    }

    Ok(())
}

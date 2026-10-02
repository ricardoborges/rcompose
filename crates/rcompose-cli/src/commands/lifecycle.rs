use crate::cli::{BuildArgs, ServiceListArgs, StopArgs};
use crate::ui;
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::engine::{BuildOptions, ContainerEngine};
use rcompose_engine::WslcEngine;

pub async fn handle_start(orchestrator: &Orchestrator<WslcEngine>, args: ServiceListArgs) -> anyhow::Result<()> {
    let targets = if args.services.is_empty() {
        None
    } else {
        Some(args.services.as_slice())
    };
    orchestrator.start(targets).await?;
    ui::success("Services started");
    Ok(())
}

pub async fn handle_stop(orchestrator: &Orchestrator<WslcEngine>, args: StopArgs) -> anyhow::Result<()> {
    let targets = if args.services.is_empty() {
        None
    } else {
        Some(args.services.as_slice())
    };
    orchestrator.stop(targets).await?;
    ui::success("Services stopped");
    Ok(())
}

pub async fn handle_restart(orchestrator: &Orchestrator<WslcEngine>, args: StopArgs) -> anyhow::Result<()> {
    let targets = if args.services.is_empty() {
        None
    } else {
        Some(args.services.as_slice())
    };
    orchestrator.restart(targets).await?;
    ui::success("Services restarted");
    Ok(())
}

pub async fn handle_build(orchestrator: &Orchestrator<WslcEngine>, args: BuildArgs) -> anyhow::Result<()> {
    let project = orchestrator.project();
    let filter = !args.services.is_empty();

    for (name, svc) in &project.services {
        if filter && !args.services.contains(name) {
            continue;
        }

        if let Some(ref build_cfg) = svc.build {
            let tag = svc.image.clone().unwrap_or_else(|| format!("{}-{}", project.name, name));
            ui::info(&format!("Building image for service '{}' ({})", name, tag));

            let build_opts = BuildOptions {
                tag,
                context: build_cfg.context.clone(),
                dockerfile: build_cfg.dockerfile.clone(),
                args: build_cfg.args.clone(),
                target: build_cfg.target.clone(),
                pull: args.pull || build_cfg.pull,
                no_cache: args.no_cache || build_cfg.no_cache,
            };

            orchestrator.engine().build_image(build_opts).await?;
            ui::success(&format!("Image built for service '{}'", name));
        }
    }

    Ok(())
}

use crate::cli::ExecArgs;
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::wslc::discovery::find_wslc;
use rcompose_engine::WslcEngine;
use std::process::Command;

pub fn handle_exec(orchestrator: &Orchestrator<WslcEngine>, args: ExecArgs) -> anyhow::Result<()> {
    let service = orchestrator
        .project()
        .services
        .get(&args.service)
        .ok_or_else(|| anyhow::anyhow!("Service '{}' not found", args.service))?;

    let container_name = orchestrator.project().container_name(service, 1);
    let wslc_bin = find_wslc().map_err(|e| anyhow::anyhow!(e))?;

    let mut cmd = Command::new(wslc_bin);
    cmd.arg("exec");

    if args.interactive {
        cmd.arg("-i");
    }
    if args.tty {
        cmd.arg("-t");
    }
    if let Some(ref u) = args.user {
        cmd.arg("-u");
        cmd.arg(u);
    }
    if let Some(ref w) = args.workdir {
        cmd.arg("-w");
        cmd.arg(w);
    }

    cmd.arg(&container_name);
    cmd.args(&args.command);

    let status = cmd.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

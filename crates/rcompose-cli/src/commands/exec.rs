use crate::cli::ExecArgs;
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::WslcEngine;
use std::io::IsTerminal;
use std::process::Command;

pub fn handle_exec(orchestrator: &Orchestrator<WslcEngine>, args: ExecArgs) -> anyhow::Result<()> {
    let project = orchestrator.project();
    let service = project
        .services
        .get(&args.service)
        .ok_or_else(|| anyhow::anyhow!("no such service: {}", args.service))?;
    let container_name = project.container_name(service, args.index);

    let mut cmd = Command::new(orchestrator.engine().binary());
    cmd.arg("exec");
    if args.detach {
        cmd.arg("-d");
    } else {
        cmd.arg("-i");
        if !args.no_tty && std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            cmd.arg("-t");
        }
    }
    if let Some(ref u) = args.user {
        cmd.args(["-u", u]);
    }
    if let Some(ref w) = args.workdir {
        cmd.args(["-w", w]);
    }
    for e in &args.env {
        cmd.args(["-e", e]);
    }
    cmd.arg(&container_name);
    cmd.args(&args.command);

    let status = cmd.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

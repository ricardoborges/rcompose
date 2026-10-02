use crate::cli::PsArgs;
use colored::Colorize;
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::WslcEngine;

pub async fn handle_ps(orchestrator: &Orchestrator<WslcEngine>, args: PsArgs) -> anyhow::Result<()> {
    let containers: Vec<_> = orchestrator
        .ps()
        .await?
        .into_iter()
        .filter(|c| args.all || c.running)
        .filter(|c| args.services.is_empty() || args.services.contains(&c.service))
        .collect();

    if args.quiet {
        for c in &containers {
            println!("{}", c.id);
        }
        return Ok(());
    }

    let name_w = containers.iter().map(|c| c.name.len()).max().unwrap_or(0).max(4);
    let image_w = containers.iter().map(|c| c.image.len()).max().unwrap_or(0).max(5);
    let service_w = containers.iter().map(|c| c.service.len()).max().unwrap_or(0).max(7);
    println!(
        "{:<name_w$}   {:<image_w$}   {:<service_w$}   {:<22}   {}",
        "NAME".bold(),
        "IMAGE".bold(),
        "SERVICE".bold(),
        "STATUS".bold(),
        "PORTS".bold()
    );

    for c in &containers {
        let status = match (&c.health, c.running) {
            (Some(h), true) => format!("{} ({})", c.state, h),
            (_, false) if c.exit_code.is_some() && c.state == "exited" => {
                format!("exited ({})", c.exit_code.unwrap_or_default())
            }
            _ => c.state.clone(),
        };
        let padded = format!("{:<22}", status);
        let status_colored = match c.health.as_deref() {
            Some("unhealthy") => padded.red(),
            _ if c.running => padded.green(),
            _ => padded.yellow(),
        };
        println!(
            "{:<name_w$}   {:<image_w$}   {:<service_w$}   {}   {}",
            c.name,
            c.image,
            c.service,
            status_colored,
            c.ports.join(", ")
        );
    }
    Ok(())
}

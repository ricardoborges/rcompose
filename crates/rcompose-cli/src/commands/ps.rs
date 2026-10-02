use crate::cli::PsArgs;
use colored::Colorize;
use rcompose_core::orchestrator::Orchestrator;
use rcompose_engine::WslcEngine;

pub async fn handle_ps(orchestrator: &Orchestrator<WslcEngine>, args: PsArgs) -> anyhow::Result<()> {
    let containers = orchestrator.ps().await?;

    if containers.is_empty() {
        println!("No containers found for project '{}'", orchestrator.project().name);
        return Ok(());
    }

    let filter_services = !args.services.is_empty();

    println!(
        "{:<25} {:<15} {:<25} {:<15} {:<20}",
        "NAME".bold(),
        "SERVICE".bold(),
        "IMAGE".bold(),
        "STATUS".bold(),
        "PORTS".bold()
    );

    for c in &containers {
        if filter_services && !args.services.contains(&c.service) {
            continue;
        }

        let status_colored = if c.status.to_lowercase().contains("run") {
            c.status.green()
        } else {
            c.status.yellow()
        };

        println!(
            "{:<25} {:<15} {:<25} {:<15} {:<20}",
            c.name,
            c.service,
            c.image,
            status_colored,
            c.ports.join(", ")
        );
    }

    Ok(())
}

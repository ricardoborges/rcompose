use crate::cli::ConfigArgs;
use rcompose_spec::model::Project;

pub fn handle_config(project: &Project, args: ConfigArgs) -> anyhow::Result<()> {
    if args.quiet {
        return Ok(());
    }

    if args.services {
        let mut names: Vec<&String> = project.services.keys().collect();
        names.sort();
        for name in names {
            println!("{}", name);
        }
        return Ok(());
    }

    if args.format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(project)?);
    } else {
        print!("{}", serde_yaml::to_string(project)?);
    }
    Ok(())
}

use crate::cli::ConfigArgs;
use rcompose_spec::model::Project;

pub fn handle_config(project: &Project, args: ConfigArgs) -> anyhow::Result<()> {
    if args.quiet {
        return Ok(());
    }

    if args.format.to_lowercase() == "json" {
        let json_str = serde_json::to_string_pretty(project)?;
        println!("{}", json_str);
    } else {
        let yaml_str = serde_yaml::to_string(project)?;
        println!("{}", yaml_str);
    }

    Ok(())
}

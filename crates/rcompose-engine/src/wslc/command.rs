//! Command-line argument vector builders for the wslc CLI.

use crate::engine::{BuildOptions, CreateOptions};
use rcompose_spec::model::{Healthcheck, VolumeType};

pub fn build_create_args(opts: &CreateOptions) -> Vec<String> {
    let mut args = vec!["create".to_string(), "--name".to_string(), opts.name.clone()];

    let mut push = |flag: &str, value: &str| {
        args.push(flag.to_string());
        args.push(value.to_string());
    };

    for (k, v) in &opts.labels {
        push("-l", &format!("{}={}", k, v));
    }
    for (k, v) in &opts.environment {
        match v {
            Some(val) => push("-e", &format!("{}={}", k, val)),
            None => push("-e", k),
        }
    }
    for port in &opts.ports {
        push("-p", &port.to_flag());
    }
    for mount in &opts.volumes {
        if mount.mount_type == VolumeType::Tmpfs {
            push("--tmpfs", &mount.target);
            continue;
        }
        let mode = if mount.read_only { ":ro" } else { "" };
        match mount.source {
            Some(ref source) => push("-v", &format!("{}:{}{}", source, mount.target, mode)),
            None => push("-v", &mount.target),
        }
    }
    for tmpfs in &opts.tmpfs {
        push("--tmpfs", tmpfs);
    }
    if let Some(ref net) = opts.network {
        push("--network", net);
        for alias in &opts.network_aliases {
            push("--network-alias", alias);
        }
    }

    let optional = [
        ("-h", &opts.hostname),
        ("--domainname", &opts.domainname),
        ("-u", &opts.user),
        ("-w", &opts.working_dir),
        ("-m", &opts.mem_limit),
        ("--cpus", &opts.cpus),
        ("--shm-size", &opts.shm_size),
        ("--stop-signal", &opts.stop_signal),
        ("--gpus", &opts.gpus),
    ];
    for (flag, value) in optional {
        if let Some(v) = value {
            push(flag, v);
        }
    }
    if let Some(t) = opts.stop_timeout {
        push("--stop-timeout", &t.to_string());
    }
    for dns in &opts.dns {
        push("--dns", dns);
    }
    for search in &opts.dns_search {
        push("--dns-search", search);
    }
    for opt in &opts.dns_opt {
        push("--dns-option", opt);
    }
    for ulimit in &opts.ulimits {
        push("--ulimit", ulimit);
    }
    if let Some(ref hc) = opts.healthcheck {
        push_healthcheck(&mut args, hc);
    }
    if opts.stdin_open {
        args.push("-i".to_string());
    }
    if opts.tty {
        args.push("-t".to_string());
    }

    // wslc takes a single executable for --entrypoint; the rest becomes leading arguments
    let entrypoint = opts.entrypoint.as_deref().unwrap_or_default();
    if let Some(first) = entrypoint.first() {
        args.push("--entrypoint".to_string());
        args.push(first.clone());
    }

    args.push(opts.image.clone());
    args.extend(entrypoint.iter().skip(1).cloned());
    if let Some(ref cmd) = opts.command {
        args.extend(cmd.iter().cloned());
    }
    args
}

fn push_healthcheck(args: &mut Vec<String>, hc: &Healthcheck) {
    if hc.disable {
        args.push("--no-healthcheck".to_string());
        return;
    }
    // wslc runs --health-cmd through the shell (CMD-SHELL); quote exec-form tests accordingly
    let cmd = match hc.test.split_first() {
        Some((kind, rest)) if kind == "CMD-SHELL" => Some(rest.join(" ")),
        Some((kind, rest)) if kind == "CMD" => Some(shell_words::join(rest)),
        Some(_) => Some(shell_words::join(&hc.test)),
        None => None,
    };
    if let Some(cmd) = cmd {
        args.push("--health-cmd".to_string());
        args.push(cmd);
    }
    let timing = [
        ("--health-interval", &hc.interval),
        ("--health-timeout", &hc.timeout),
        ("--health-start-period", &hc.start_period),
    ];
    for (flag, value) in timing {
        if let Some(v) = value {
            args.push(flag.to_string());
            args.push(v.clone());
        }
    }
    if let Some(retries) = hc.retries {
        args.push("--health-retries".to_string());
        args.push(retries.to_string());
    }
}

pub fn build_build_args(opts: &BuildOptions) -> Vec<String> {
    let mut args = vec!["build".to_string(), "-t".to_string(), opts.tag.clone()];

    if let Some(ref df) = opts.dockerfile {
        args.push("-f".to_string());
        args.push(df.clone());
    }
    for (k, v) in &opts.args {
        args.push("--build-arg".to_string());
        args.push(format!("{}={}", k, v));
    }
    if let Some(ref target) = opts.target {
        args.push("--target".to_string());
        args.push(target.clone());
    }
    if opts.pull {
        args.push("--pull".to_string());
    }
    if opts.no_cache {
        args.push("--no-cache".to_string());
    }

    args.push(opts.context.clone());
    args
}

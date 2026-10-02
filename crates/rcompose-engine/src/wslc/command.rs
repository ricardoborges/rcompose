//! Command-line argument vector builders for the wslc CLI.

use crate::engine::{BuildOptions, RunOptions};
use crate::wslc::paths::to_host_path;
use rcompose_spec::model::VolumeType;

pub fn build_run_args(opts: &RunOptions) -> Vec<String> {
    let mut args = vec!["run".to_string()];

    if opts.detach {
        args.push("-d".to_string());
    }

    if let Some(ref session) = opts.wsl_session {
        args.push("--session".to_string());
        args.push(session.clone());
    }

    args.push("--name".to_string());
    args.push(opts.name.clone());

    for (k, v) in &opts.labels {
        args.push("-l".to_string());
        args.push(format!("{}={}", k, v));
    }

    for env_file in &opts.env_files {
        args.push("--env-file".to_string());
        args.push(to_host_path(env_file));
    }

    for (k, v) in &opts.environment {
        args.push("-e".to_string());
        if let Some(val) = v {
            args.push(format!("{}={}", k, val));
        } else {
            args.push(k.clone());
        }
    }

    for port in &opts.ports {
        args.push("-p".to_string());
        args.push(port.to_flag());
    }

    for mount in &opts.volumes {
        if mount.mount_type == VolumeType::Tmpfs {
            args.push("--tmpfs".to_string());
            args.push(mount.target.clone());
            continue;
        }

        let source = mount.source.as_deref().unwrap_or("");
        let host_source = if mount.mount_type == VolumeType::Bind {
            to_host_path(source)
        } else {
            source.to_string()
        };

        let mut spec = format!("{}:{}", host_source, mount.target);
        if mount.read_only {
            spec.push_str(":ro");
        }
        args.push("-v".to_string());
        args.push(spec);
    }

    for tmpfs in &opts.tmpfs {
        args.push("--tmpfs".to_string());
        args.push(tmpfs.clone());
    }

    if let Some(net) = opts.networks.first() {
        args.push("--network".to_string());
        args.push(net.clone());

        if let Some(aliases) = opts.network_aliases.get(net) {
            for alias in aliases {
                args.push("--network-alias".to_string());
                args.push(alias.clone());
            }
        }
    }

    if let Some(ref h) = opts.hostname {
        args.push("-h".to_string());
        args.push(h.clone());
    }
    if let Some(ref d) = opts.domainname {
        args.push("--domainname".to_string());
        args.push(d.clone());
    }

    for dns in &opts.dns {
        args.push("--dns".to_string());
        args.push(dns.clone());
    }
    for search in &opts.dns_search {
        args.push("--dns-search".to_string());
        args.push(search.clone());
    }
    for opt in &opts.dns_opt {
        args.push("--dns-option".to_string());
        args.push(opt.clone());
    }

    if let Some(ref u) = opts.user {
        args.push("-u".to_string());
        args.push(u.clone());
    }
    if let Some(ref w) = opts.working_dir {
        args.push("-w".to_string());
        args.push(w.clone());
    }
    if let Some(ref m) = opts.mem_limit {
        args.push("-m".to_string());
        args.push(m.clone());
    }
    if let Some(ref c) = opts.cpus {
        args.push("--cpus".to_string());
        args.push(c.clone());
    }
    if let Some(ref s) = opts.shm_size {
        args.push("--shm-size".to_string());
        args.push(s.clone());
    }
    for ulimit in &opts.ulimits {
        args.push("--ulimit".to_string());
        args.push(ulimit.clone());
    }
    if let Some(ref sig) = opts.stop_signal {
        args.push("--stop-signal".to_string());
        args.push(sig.clone());
    }
    if let Some(ref g) = opts.gpus {
        args.push("--gpus".to_string());
        args.push(g.clone());
    }
    if opts.stdin_open {
        args.push("-i".to_string());
    }
    if opts.tty {
        args.push("-t".to_string());
    }

    if let Some(ref ep) = opts.entrypoint {
        if let Some(first) = ep.first() {
            args.push("--entrypoint".to_string());
            args.push(first.clone());
        }
    }

    args.push(opts.image.clone());

    if let Some(ref ep) = opts.entrypoint {
        if ep.len() > 1 {
            args.extend(ep[1..].iter().cloned());
        }
    }

    if let Some(ref cmd) = opts.command {
        args.extend(cmd.iter().cloned());
    }

    args
}

pub fn build_build_args(opts: &BuildOptions) -> Vec<String> {
    let mut args = vec!["build".to_string(), "-t".to_string(), opts.tag.clone()];

    if let Some(ref df) = opts.dockerfile {
        args.push("-f".to_string());
        args.push(to_host_path(df));
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

    args.push(to_host_path(&opts.context));
    args
}

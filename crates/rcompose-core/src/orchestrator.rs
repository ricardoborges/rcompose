//! Orchestrator coordinating Compose service lifecycles across the dependency graph and the ContainerEngine.

use crate::dag::{DagError, DependencyGraph};
use crate::drift::{compute_config_hash, reconcile_service_state, DesiredAction};
use rcompose_engine::engine::*;
use rcompose_spec::model::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::watch;

#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("DAG error: {0}")]
    Dag(#[from] DagError),
    #[error("Engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("{0}")]
    Custom(String),
}

#[derive(Debug, Clone, Default)]
pub struct UpOptions {
    pub services: Option<Vec<String>>,
    pub force_recreate: bool,
    /// Build images even when they already exist.
    pub build: bool,
    /// Never build; services whose image is missing fail.
    pub no_build: bool,
    pub remove_orphans: bool,
    /// Stop timeout used when recreating containers.
    pub timeout: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct DownOptions {
    pub remove_volumes: bool,
    pub remove_orphans: bool,
    pub timeout: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileEvent {
    pub service: String,
    pub container_name: String,
    pub action: DesiredAction,
}

/// A progress line such as `Container web-1  Started`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressEvent {
    /// e.g. `Container painkiller-api`, `Network painkiller_default`.
    pub resource: String,
    pub status: String,
    pub kind: ProgressKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    Working,
    Done,
    Warning,
}

pub type Reporter = Arc<dyn Fn(ProgressEvent) + Send + Sync>;

const POLL_INTERVAL: Duration = Duration::from_millis(1000);

pub struct Orchestrator<E: ContainerEngine> {
    project: Arc<Project>,
    engine: Arc<E>,
    reporter: Reporter,
}

impl<E: ContainerEngine> Clone for Orchestrator<E> {
    fn clone(&self) -> Self {
        Self {
            project: Arc::clone(&self.project),
            engine: Arc::clone(&self.engine),
            reporter: Arc::clone(&self.reporter),
        }
    }
}

impl<E: ContainerEngine + 'static> Orchestrator<E> {
    pub fn new(project: Project, engine: E) -> Self {
        Self {
            project: Arc::new(project),
            engine: Arc::new(engine),
            reporter: Arc::new(|_| {}),
        }
    }

    /// Receives a [`ProgressEvent`] for every step taken.
    pub fn with_reporter(mut self, reporter: Reporter) -> Self {
        self.reporter = reporter;
        self
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn engine(&self) -> &E {
        &self.engine
    }

    fn report(&self, resource: impl Into<String>, status: impl Into<String>, kind: ProgressKind) {
        (self.reporter)(ProgressEvent { resource: resource.into(), status: status.into(), kind });
    }

    /// Selected services (plus their dependencies) in dependency order.
    fn ordered_services(&self, selected: Option<&[String]>) -> Result<Vec<Service>, OrchestratorError> {
        let dag = DependencyGraph::from_project_and_services(&self.project, selected)?;
        Ok(dag
            .execution_batches()
            .iter()
            .flatten()
            .filter_map(|name| self.project.services.get(name).cloned())
            .collect())
    }

    /// Brings up the project services, honoring `depends_on` conditions.
    pub async fn up(&self, opts: UpOptions) -> Result<Vec<ReconcileEvent>, OrchestratorError> {
        let services = self.ordered_services(opts.services.as_deref())?;

        self.ensure_networks(&services).await?;
        self.ensure_volumes(&services).await?;
        let rebuilt = self.prepare_images(&services, &opts).await?;

        let existing = self.engine.list_containers(&self.project.name).await?;
        self.handle_orphans(&existing, opts.remove_orphans).await?;
        self.remove_surplus_replicas(&existing, &services).await?;
        let existing: Arc<HashMap<String, ContainerDetails>> =
            Arc::new(existing.into_iter().map(|c| (c.name.clone(), c)).collect());

        // One task per service; each waits for its dependencies through a watch channel.
        let mut senders = HashMap::new();
        let mut receivers = HashMap::new();
        for svc in &services {
            let (tx, rx) = watch::channel::<Option<Result<(), String>>>(None);
            senders.insert(svc.name.clone(), tx);
            receivers.insert(svc.name.clone(), rx);
        }

        let mut handles = Vec::new();
        for svc in services {
            let this = self.clone();
            let tx = senders.remove(&svc.name).expect("sender exists");
            let deps: Vec<(String, watch::Receiver<_>)> = svc
                .depends_on
                .iter()
                .filter_map(|d| receivers.get(d).map(|rx| (d.clone(), rx.clone())))
                .collect();
            let existing = Arc::clone(&existing);
            let force = opts.force_recreate || rebuilt.contains(&self.project.image_name(&svc));
            let timeout = opts.timeout;

            handles.push(tokio::spawn(async move {
                let result = async {
                    for (dep, mut rx) in deps {
                        let state = rx
                            .wait_for(Option::is_some)
                            .await
                            .map_err(|_| OrchestratorError::Custom(format!("dependency '{}' was aborted", dep)))?
                            .clone();
                        if let Some(Err(e)) = state {
                            return Err(OrchestratorError::Custom(format!("dependency '{}' failed to start: {}", dep, e)));
                        }
                        this.wait_for_condition(&dep, svc.dependency_condition(&dep)).await?;
                    }
                    this.up_service(&svc, &existing, force, timeout).await
                }
                .await;
                let _ = tx.send(Some(result.as_ref().map(|_| ()).map_err(|e| e.to_string())));
                result
            }));
        }

        let mut events = Vec::new();
        let mut first_error = None;
        for handle in handles {
            match handle.await {
                Ok(Ok(svc_events)) => events.extend(svc_events),
                Ok(Err(err)) => {
                    first_error.get_or_insert(err);
                }
                Err(join_err) => {
                    first_error.get_or_insert(OrchestratorError::Custom(join_err.to_string()));
                }
            }
        }
        match first_error {
            Some(err) => Err(err),
            None => Ok(events),
        }
    }

    async fn up_service(
        &self,
        svc: &Service,
        existing: &HashMap<String, ContainerDetails>,
        force_recreate: bool,
        timeout: Option<u32>,
    ) -> Result<Vec<ReconcileEvent>, OrchestratorError> {
        let mut events = Vec::new();
        for idx in 1..=svc.replicas.max(1) {
            let cname = self.project.container_name(svc, idx);
            let resource = format!("Container {}", cname);
            let action = reconcile_service_state(svc, existing.get(&cname), force_recreate);

            match action {
                DesiredAction::Create => {
                    self.report(&resource, "Creating", ProgressKind::Working);
                    self.create_and_start(svc, idx).await?;
                    self.report(&resource, "Started", ProgressKind::Done);
                }
                DesiredAction::Recreate => {
                    self.report(&resource, "Recreating", ProgressKind::Working);
                    let stop_timeout = timeout.or(svc.stop_grace_period);
                    if existing.get(&cname).is_some_and(|c| c.running) {
                        self.engine.stop_container(&cname, stop_timeout).await?;
                    }
                    self.engine.remove_container(&cname, true).await?;
                    self.create_and_start(svc, idx).await?;
                    self.report(&resource, "Recreated", ProgressKind::Done);
                }
                DesiredAction::Start => {
                    self.report(&resource, "Starting", ProgressKind::Working);
                    self.engine.start_container(&cname).await?;
                    self.report(&resource, "Started", ProgressKind::Done);
                }
                DesiredAction::UpToDate => {
                    self.report(&resource, "Running", ProgressKind::Done);
                }
            }

            events.push(ReconcileEvent { service: svc.name.clone(), container_name: cname, action });
        }
        Ok(events)
    }

    async fn create_and_start(&self, svc: &Service, idx: usize) -> Result<(), OrchestratorError> {
        let opts = make_create_options(&self.project, svc, idx);
        let name = opts.name.clone();
        self.engine.create_container(opts).await?;
        for net in svc.networks.iter().skip(1) {
            self.engine.connect_network(net, &name, &network_aliases(svc, net)).await?;
        }
        self.engine.start_container(&name).await?;
        Ok(())
    }

    /// Blocks until every container of `dependency` satisfies `condition`.
    async fn wait_for_condition(&self, dependency: &str, condition: DependencyCondition) -> Result<(), OrchestratorError> {
        if condition == DependencyCondition::ServiceStarted {
            return Ok(());
        }
        let svc = self
            .project
            .services
            .get(dependency)
            .ok_or_else(|| OrchestratorError::Custom(format!("unknown dependency '{}'", dependency)))?;

        for idx in 1..=svc.replicas.max(1) {
            let cname = self.project.container_name(svc, idx);
            let resource = format!("Container {}", cname);
            self.report(&resource, "Waiting", ProgressKind::Working);
            // health may not be reported right after start, unless the service declares no healthcheck
            let mut grace_polls = if svc.healthcheck.as_ref().is_some_and(|h| !h.disable) { u32::MAX } else { 5 };
            loop {
                let c = self
                    .engine
                    .inspect_container(&cname)
                    .await?
                    .ok_or_else(|| OrchestratorError::Custom(format!("container {} disappeared", cname)))?;
                let fail = |why: String| OrchestratorError::Custom(format!("container {} {}", cname, why));

                match condition {
                    DependencyCondition::ServiceHealthy => match c.health.as_deref() {
                        Some("healthy") => {
                            self.report(&resource, "Healthy", ProgressKind::Done);
                            break;
                        }
                        Some("unhealthy") => return Err(fail("is unhealthy".into())),
                        None if c.running && grace_polls == 0 => {
                            return Err(fail("has no healthcheck configured".into()))
                        }
                        None if c.running => grace_polls -= 1,
                        _ if !c.running => {
                            return Err(fail(format!("exited with code {}", c.exit_code.unwrap_or(-1))))
                        }
                        _ => {}
                    },
                    DependencyCondition::ServiceCompletedSuccessfully => {
                        if !c.running && c.state != "created" {
                            match c.exit_code {
                                Some(0) => {
                                    self.report(&resource, "Exited", ProgressKind::Done);
                                    break;
                                }
                                code => {
                                    return Err(fail(format!("exited with code {}", code.unwrap_or(-1))))
                                }
                            }
                        }
                    }
                    DependencyCondition::ServiceStarted => unreachable!(),
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }
        Ok(())
    }

    async fn ensure_networks(&self, services: &[Service]) -> Result<(), OrchestratorError> {
        let used: HashSet<&String> = services.iter().flat_map(|s| &s.networks).collect();
        let existing = self.engine.list_networks().await?;
        for net in self.project.networks.values().filter(|n| used.contains(&n.name)) {
            if existing.contains(&net.name) {
                continue;
            }
            if net.external {
                return Err(OrchestratorError::Custom(format!(
                    "network {} declared as external, but could not be found (create it with: wslc network create {})",
                    net.name, net.name
                )));
            }
            let labels = BTreeMap::from([
                (LABEL_PROJECT.to_string(), self.project.name.clone()),
                (LABEL_NETWORK.to_string(), net.key.clone()),
            ]);
            self.engine.create_network(&net.name, &labels).await?;
            self.report(format!("Network {}", net.name), "Created", ProgressKind::Done);
        }
        Ok(())
    }

    async fn ensure_volumes(&self, services: &[Service]) -> Result<(), OrchestratorError> {
        let used: HashSet<&str> = services
            .iter()
            .flat_map(|s| &s.volumes)
            .filter(|m| m.mount_type == VolumeType::Volume)
            .filter_map(|m| m.source.as_deref())
            .collect();
        let existing = self.engine.list_volumes().await?;
        for vol in self.project.volumes.values().filter(|v| used.contains(v.name.as_str())) {
            if existing.contains(&vol.name) {
                continue;
            }
            if vol.external {
                return Err(OrchestratorError::Custom(format!(
                    "volume {} declared as external, but could not be found",
                    vol.name
                )));
            }
            let labels = BTreeMap::from([
                (LABEL_PROJECT.to_string(), self.project.name.clone()),
                (LABEL_VOLUME.to_string(), vol.key.clone()),
            ]);
            self.engine.create_volume(&vol.name, &labels).await?;
            self.report(format!("Volume {}", vol.name), "Created", ProgressKind::Done);
        }
        Ok(())
    }

    /// Builds or pulls missing images; returns the images built in this run.
    async fn prepare_images(&self, services: &[Service], opts: &UpOptions) -> Result<HashSet<String>, OrchestratorError> {
        let mut seen = HashSet::new();
        let mut built = HashSet::new();
        for svc in services {
            let image = self.project.image_name(svc);
            if !seen.insert(image.clone()) {
                continue;
            }
            let exists = self.engine.image_exists(&image).await?;
            if svc.build.is_some() && !opts.no_build && (opts.build || !exists) {
                self.build_service(svc, false, false).await?;
                built.insert(image);
            } else if !exists {
                if svc.build.is_some() {
                    return Err(OrchestratorError::Custom(format!(
                        "image {} for service {} is missing and --no-build was given",
                        image, svc.name
                    )));
                }
                let resource = format!("Image {}", image);
                self.report(&resource, "Pulling", ProgressKind::Working);
                self.engine.pull_image(&image).await?;
                self.report(&resource, "Pulled", ProgressKind::Done);
            }
        }
        Ok(built)
    }

    /// Builds the image of one service.
    pub async fn build_service(&self, svc: &Service, pull: bool, no_cache: bool) -> Result<(), OrchestratorError> {
        let Some(ref build_cfg) = svc.build else {
            return Ok(());
        };
        let tag = self.project.image_name(svc);
        let resource = format!("Image {}", tag);
        self.report(&resource, "Building", ProgressKind::Working);
        self.engine
            .build_image(BuildOptions {
                tag,
                context: build_cfg.context.clone(),
                dockerfile: build_cfg.dockerfile.clone(),
                args: build_cfg.args.clone(),
                target: build_cfg.target.clone(),
                pull: pull || build_cfg.pull,
                no_cache: no_cache || build_cfg.no_cache,
            })
            .await?;
        self.report(&resource, "Built", ProgressKind::Done);
        Ok(())
    }

    /// Builds every selected service that has a `build` section.
    pub async fn build(&self, services: Option<&[String]>, pull: bool, no_cache: bool) -> Result<(), OrchestratorError> {
        let mut targets: Vec<&Service> = self
            .project
            .services
            .values()
            .filter(|s| s.build.is_some())
            .filter(|s| services.is_none_or(|names| names.contains(&s.name)))
            .collect();
        if let Some(names) = services {
            if let Some(unknown) = names.iter().find(|n| !self.project.services.contains_key(*n)) {
                return Err(OrchestratorError::Dag(DagError::ServiceNotFound(unknown.clone())));
            }
        }
        targets.sort_by(|a, b| a.name.cmp(&b.name));
        let mut done = HashSet::new();
        for svc in targets {
            if done.insert(self.project.image_name(svc)) {
                self.build_service(svc, pull, no_cache).await?;
            }
        }
        Ok(())
    }

    /// Pulls the images of services that do not build their own.
    pub async fn pull(&self, services: Option<&[String]>) -> Result<(), OrchestratorError> {
        let mut seen = HashSet::new();
        for svc in self.ordered_services(services)? {
            if svc.build.is_some() || !seen.insert(self.project.image_name(&svc)) {
                continue;
            }
            let image = self.project.image_name(&svc);
            let resource = format!("Image {}", image);
            self.report(&resource, "Pulling", ProgressKind::Working);
            self.engine.pull_image(&image).await?;
            self.report(&resource, "Pulled", ProgressKind::Done);
        }
        Ok(())
    }

    fn is_known_service(&self, service: &str) -> bool {
        self.project.services.contains_key(service) || self.project.disabled_services.contains(service)
    }

    async fn handle_orphans(&self, existing: &[ContainerDetails], remove: bool) -> Result<(), OrchestratorError> {
        let orphans: Vec<&ContainerDetails> = existing.iter().filter(|c| !self.is_known_service(&c.service)).collect();
        if orphans.is_empty() {
            return Ok(());
        }
        if !remove {
            let names: Vec<&str> = orphans.iter().map(|c| c.name.as_str()).collect();
            self.report(
                format!("Project {}", self.project.name),
                format!(
                    "Found orphan containers ({}). Run with --remove-orphans to clean them up.",
                    names.join(", ")
                ),
                ProgressKind::Warning,
            );
            return Ok(());
        }
        for c in orphans {
            self.remove_container(c, None).await?;
        }
        Ok(())
    }

    async fn remove_surplus_replicas(&self, existing: &[ContainerDetails], services: &[Service]) -> Result<(), OrchestratorError> {
        for svc in services {
            for c in existing.iter().filter(|c| c.service == svc.name && c.number > svc.replicas.max(1)) {
                self.remove_container(c, svc.stop_grace_period).await?;
            }
        }
        Ok(())
    }

    async fn remove_container(&self, c: &ContainerDetails, timeout: Option<u32>) -> Result<(), OrchestratorError> {
        let resource = format!("Container {}", c.name);
        if c.running {
            self.report(&resource, "Stopping", ProgressKind::Working);
            self.engine.stop_container(&c.name, timeout).await?;
        }
        self.report(&resource, "Removing", ProgressKind::Working);
        self.engine.remove_container(&c.name, true).await?;
        self.report(&resource, "Removed", ProgressKind::Done);
        Ok(())
    }

    /// Project containers grouped in dependency order (dependencies first).
    async fn containers_in_order(&self, services: Option<&[String]>) -> Result<Vec<Vec<ContainerDetails>>, OrchestratorError> {
        let dag = DependencyGraph::from_project_and_services(&self.project, services)?;
        let mut by_service: HashMap<String, Vec<ContainerDetails>> = HashMap::new();
        for c in self.engine.list_containers(&self.project.name).await? {
            by_service.entry(c.service.clone()).or_default().push(c);
        }

        let mut batches: Vec<Vec<ContainerDetails>> = Vec::new();
        for batch in dag.execution_batches() {
            let wanted = batch.iter().filter(|s| services.is_none_or(|names| names.contains(s)));
            batches.push(wanted.flat_map(|s| by_service.remove(s).unwrap_or_default()).collect());
        }
        if services.is_none() {
            // containers of profile-disabled services go last
            let rest: Vec<ContainerDetails> = by_service
                .into_iter()
                .filter(|(s, _)| self.project.disabled_services.contains(s))
                .flat_map(|(_, cs)| cs)
                .collect();
            batches.push(rest);
        }
        Ok(batches)
    }

    /// Tears down containers, networks, and optionally volumes.
    pub async fn down(&self, opts: DownOptions) -> Result<(), OrchestratorError> {
        let mut batches = self.containers_in_order(None).await?;
        batches.reverse();
        for batch in batches {
            let mut handles = Vec::new();
            for c in batch {
                let this = self.clone();
                let timeout = opts.timeout.or_else(|| self.project.services.get(&c.service).and_then(|s| s.stop_grace_period));
                handles.push(tokio::spawn(async move { this.remove_container(&c, timeout).await }));
            }
            for handle in handles {
                handle.await.map_err(|e| OrchestratorError::Custom(e.to_string()))??;
            }
        }

        let existing = self.engine.list_containers(&self.project.name).await?;
        self.handle_orphans(&existing, opts.remove_orphans).await?;

        let networks = self.engine.list_networks().await?;
        for net in self.project.networks.values().filter(|n| !n.external && networks.contains(&n.name)) {
            let resource = format!("Network {}", net.name);
            match self.engine.remove_network(&net.name).await {
                Ok(()) => self.report(&resource, "Removed", ProgressKind::Done),
                Err(e) => self.report(&resource, format!("could not be removed: {}", e), ProgressKind::Warning),
            }
        }

        if opts.remove_volumes {
            let volumes = self.engine.list_volumes().await?;
            for vol in self.project.volumes.values().filter(|v| !v.external && volumes.contains(&v.name)) {
                let resource = format!("Volume {}", vol.name);
                match self.engine.remove_volume(&vol.name).await {
                    Ok(()) => self.report(&resource, "Removed", ProgressKind::Done),
                    Err(e) => self.report(&resource, format!("could not be removed: {}", e), ProgressKind::Warning),
                }
            }
        }
        Ok(())
    }

    /// Lists project containers, sorted by name.
    pub async fn ps(&self) -> Result<Vec<ContainerDetails>, OrchestratorError> {
        let mut containers = self.engine.list_containers(&self.project.name).await?;
        containers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(containers)
    }

    /// Stops running containers of the given services (all when `None`), dependents first.
    pub async fn stop(&self, services: Option<&[String]>, timeout: Option<u32>) -> Result<(), OrchestratorError> {
        let mut batches = self.containers_in_order(services).await?;
        batches.reverse();
        for c in batches.into_iter().flatten().filter(|c| c.running) {
            let resource = format!("Container {}", c.name);
            self.report(&resource, "Stopping", ProgressKind::Working);
            let timeout = timeout.or_else(|| self.project.services.get(&c.service).and_then(|s| s.stop_grace_period));
            self.engine.stop_container(&c.name, timeout).await?;
            self.report(&resource, "Stopped", ProgressKind::Done);
        }
        Ok(())
    }

    /// Starts existing containers of the given services (all when `None`), dependencies first.
    pub async fn start(&self, services: Option<&[String]>) -> Result<(), OrchestratorError> {
        let batches = self.containers_in_order(services).await?;
        if batches.iter().all(Vec::is_empty) {
            return Err(OrchestratorError::Custom("no containers to start; run 'rcompose up' first".into()));
        }
        for c in batches.into_iter().flatten().filter(|c| !c.running) {
            if !self.project.services.contains_key(&c.service) {
                continue;
            }
            let resource = format!("Container {}", c.name);
            self.report(&resource, "Starting", ProgressKind::Working);
            self.engine.start_container(&c.name).await?;
            self.report(&resource, "Started", ProgressKind::Done);
        }
        Ok(())
    }

    /// Restarts project services.
    pub async fn restart(&self, services: Option<&[String]>, timeout: Option<u32>) -> Result<(), OrchestratorError> {
        self.stop(services, timeout).await?;
        self.start(services).await
    }
}

/// Aliases a service answers to on `network`: its own name plus any configured aliases.
fn network_aliases(svc: &Service, network: &str) -> Vec<String> {
    let mut aliases = vec![svc.name.clone()];
    for alias in svc.network_aliases.get(network).into_iter().flatten() {
        if !aliases.contains(alias) {
            aliases.push(alias.clone());
        }
    }
    aliases
}

fn make_create_options(project: &Project, service: &Service, index: usize) -> CreateOptions {
    let mut labels = service.labels.clone();
    labels.insert(LABEL_PROJECT.to_string(), project.name.clone());
    labels.insert(LABEL_SERVICE.to_string(), service.name.clone());
    labels.insert(LABEL_INDEX.to_string(), index.to_string());
    labels.insert(LABEL_CONFIG_HASH.to_string(), compute_config_hash(service));

    let network = service.networks.first().cloned().unwrap_or_else(|| project.default_network_name());

    CreateOptions {
        name: project.container_name(service, index),
        image: project.image_name(service),
        command: service.command.clone(),
        entrypoint: service.entrypoint.clone(),
        environment: service.environment.clone(),
        ports: service.ports.clone(),
        volumes: service.volumes.clone(),
        tmpfs: service.tmpfs.clone(),
        network_aliases: network_aliases(service, &network),
        network: Some(network),
        labels,
        hostname: service.hostname.clone(),
        domainname: service.domainname.clone(),
        dns: service.dns.clone(),
        dns_search: service.dns_search.clone(),
        dns_opt: service.dns_opt.clone(),
        user: service.user.clone(),
        working_dir: service.working_dir.clone(),
        mem_limit: service.mem_limit.clone(),
        cpus: service.cpus.clone(),
        shm_size: service.shm_size.clone(),
        ulimits: service.ulimits.clone(),
        stop_signal: service.stop_signal.clone(),
        stop_timeout: service.stop_grace_period,
        gpus: service.gpus.clone(),
        healthcheck: service.healthcheck.clone(),
        stdin_open: service.stdin_open,
        tty: service.tty,
    }
}

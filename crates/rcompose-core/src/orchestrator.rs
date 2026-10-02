//! Orchestrator coordinating Compose service lifecycles across DAG layers and the ContainerEngine.

use crate::dag::{DagError, DependencyGraph};
use crate::drift::{compute_config_hash, reconcile_service_state, DesiredAction};
use rcompose_engine::engine::*;
use rcompose_spec::model::*;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("DAG error: {0}")]
    Dag(#[from] DagError),
    #[error("Engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("Orchestrator error: {0}")]
    Custom(String),
}

#[derive(Debug, Clone, Default)]
pub struct UpOptions {
    pub services: Option<Vec<String>>,
    pub force_recreate: bool,
    pub no_build: bool,
    pub remove_orphans: bool,
}

#[derive(Debug, Clone)]
pub struct DownOptions {
    pub remove_volumes: bool,
    pub remove_orphans: bool,
    pub timeout_secs: u32,
}

impl Default for DownOptions {
    fn default() -> Self {
        Self {
            remove_volumes: false,
            remove_orphans: false,
            timeout_secs: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileEvent {
    pub service: String,
    pub container_name: String,
    pub action: DesiredAction,
}

#[derive(Clone)]
pub struct Orchestrator<E: ContainerEngine> {
    project: Arc<Project>,
    engine: Arc<E>,
}

impl<E: ContainerEngine + 'static> Orchestrator<E> {
    pub fn new(project: Project, engine: E) -> Self {
        Self {
            project: Arc::new(project),
            engine: Arc::new(engine),
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn engine(&self) -> &E {
        &self.engine
    }

    /// Brings up the project services according to DAG dependencies.
    pub async fn up(&self, opts: UpOptions) -> Result<Vec<ReconcileEvent>, OrchestratorError> {
        let dag = DependencyGraph::from_project_and_services(
            &self.project,
            opts.services.as_deref(),
        )?;

        // 1. Ensure project networks
        let default_net = format!("{}_default", self.project.name);
        self.engine.create_network(&default_net).await.ok();
        for net in self.project.networks.values() {
            self.engine.create_network(&net.name).await.ok();
        }

        // 2. Ensure volumes
        for vol in self.project.volumes.values() {
            self.engine.create_volume(&vol.name).await.ok();
        }

        let mut events = Vec::new();

        // 3. Process execution batches in DAG order
        for batch in dag.execution_batches() {
            let mut join_handles = Vec::new();

            for svc_name in batch {
                let service = match self.project.services.get(svc_name) {
                    Some(s) => s.clone(),
                    None => continue,
                };

                let project = Arc::clone(&self.project);
                let engine = Arc::clone(&self.engine);
                let force_recreate = opts.force_recreate;

                let handle = tokio::spawn(async move {
                    let mut svc_events = Vec::new();
                    for idx in 1..=service.replicas {
                        let cname = project.container_name(&service, idx);
                        let existing = engine.inspect_container(&cname).await?;
                        let action = reconcile_service_state(&service, existing.as_ref(), force_recreate);

                        match action {
                            DesiredAction::Create => {
                                let run_opts = make_run_options(&project, &service, idx);
                                engine.run_container(run_opts).await?;
                            }
                            DesiredAction::Start => {
                                engine.start_container(&cname).await?;
                            }
                            DesiredAction::Recreate => {
                                engine.stop_container(&cname, 5).await.ok();
                                engine.remove_container(&cname, true).await.ok();
                                let run_opts = make_run_options(&project, &service, idx);
                                engine.run_container(run_opts).await?;
                            }
                            DesiredAction::UpToDate => {}
                        }

                        svc_events.push(ReconcileEvent {
                            service: service.name.clone(),
                            container_name: cname,
                            action,
                        });
                    }
                    Ok::<Vec<ReconcileEvent>, OrchestratorError>(svc_events)
                });

                join_handles.push(handle);
            }

            for handle in join_handles {
                match handle.await {
                    Ok(Ok(batch_events)) => events.extend(batch_events),
                    Ok(Err(err)) => return Err(err),
                    Err(join_err) => return Err(OrchestratorError::Custom(join_err.to_string())),
                }
            }
        }

        Ok(events)
    }

    /// Tears down containers, networks, and optionally volumes.
    pub async fn down(&self, opts: DownOptions) -> Result<(), OrchestratorError> {
        let dag = DependencyGraph::from_project(&self.project)?;

        // Stop & remove containers in reverse DAG order
        for batch in dag.shutdown_batches() {
            let mut join_handles = Vec::new();

            for svc_name in batch {
                let service = match self.project.services.get(&svc_name) {
                    Some(s) => s.clone(),
                    None => continue,
                };
                let project = Arc::clone(&self.project);
                let engine = Arc::clone(&self.engine);
                let timeout = opts.timeout_secs;

                let handle = tokio::spawn(async move {
                    for idx in 1..=service.replicas {
                        let cname = project.container_name(&service, idx);
                        engine.stop_container(&cname, timeout).await.ok();
                        engine.remove_container(&cname, true).await.ok();
                    }
                    Ok::<(), OrchestratorError>(())
                });

                join_handles.push(handle);
            }

            for handle in join_handles {
                handle.await.map_err(|e| OrchestratorError::Custom(e.to_string()))??;
            }
        }

        // Remove volumes if requested
        if opts.remove_volumes {
            for vol in self.project.volumes.values() {
                // Ignore errors if volume cannot be deleted immediately
                let _ = self.engine.create_volume(&vol.name).await;
            }
        }

        Ok(())
    }

    /// Lists project containers.
    pub async fn ps(&self) -> Result<Vec<ContainerSummary>, OrchestratorError> {
        let containers = self.engine.list_containers(&self.project.name).await?;
        Ok(containers)
    }

    /// Stops project services.
    pub async fn stop(&self, services: Option<&[String]>) -> Result<(), OrchestratorError> {
        let dag = DependencyGraph::from_project_and_services(&self.project, services)?;
        for batch in dag.shutdown_batches() {
            for svc_name in batch {
                if let Some(service) = self.project.services.get(&svc_name) {
                    for idx in 1..=service.replicas {
                        let cname = self.project.container_name(service, idx);
                        self.engine.stop_container(&cname, 10).await.ok();
                    }
                }
            }
        }
        Ok(())
    }

    /// Starts project services.
    pub async fn start(&self, services: Option<&[String]>) -> Result<(), OrchestratorError> {
        let dag = DependencyGraph::from_project_and_services(&self.project, services)?;
        for batch in dag.execution_batches() {
            for svc_name in batch {
                if let Some(service) = self.project.services.get(svc_name) {
                    for idx in 1..=service.replicas {
                        let cname = self.project.container_name(service, idx);
                        self.engine.start_container(&cname).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Restarts project services.
    pub async fn restart(&self, services: Option<&[String]>) -> Result<(), OrchestratorError> {
        self.stop(services).await?;
        self.start(services).await?;
        Ok(())
    }
}

fn make_run_options(project: &Project, service: &Service, index: usize) -> RunOptions {
    let mut labels = service.labels.clone();
    labels.insert(LABEL_PROJECT.to_string(), project.name.clone());
    labels.insert(LABEL_SERVICE.to_string(), service.name.clone());
    labels.insert(LABEL_INDEX.to_string(), index.to_string());
    labels.insert(LABEL_CONFIG_HASH.to_string(), compute_config_hash(service));

    let image = service
        .image
        .clone()
        .unwrap_or_else(|| format!("{}-{}", project.name, service.name));

    let mut networks = service.networks.clone();
    if networks.is_empty() {
        networks.push(format!("{}_default", project.name));
    }

    RunOptions {
        name: project.container_name(service, index),
        image,
        detach: true,
        command: service.command.clone(),
        entrypoint: service.entrypoint.clone(),
        environment: service.environment.clone(),
        env_files: service.env_files.clone(),
        ports: service.ports.clone(),
        volumes: service.volumes.clone(),
        tmpfs: service.tmpfs.clone(),
        networks,
        network_aliases: service.network_aliases.clone(),
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
        gpus: service.gpus.clone(),
        stdin_open: service.stdin_open,
        tty: service.tty,
        wsl_session: service.wsl_session.clone(),
    }
}

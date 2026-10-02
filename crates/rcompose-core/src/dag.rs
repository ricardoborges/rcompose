//! Directed Acyclic Graph (DAG) for dependency resolution, cycle detection, and concurrent execution batches.

use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use rcompose_spec::model::Project;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum DagError {
    #[error("circular dependency detected: {0}")]
    CircularDependency(String),
    #[error("service '{0}' not found in project")]
    ServiceNotFound(String),
}

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub batches: Vec<Vec<String>>,
}

impl DependencyGraph {
    pub fn from_project(project: &Project) -> Result<Self, DagError> {
        Self::from_project_and_services(project, None)
    }

    pub fn from_project_and_services(
        project: &Project,
        selected_services: Option<&[String]>,
    ) -> Result<Self, DagError> {
        // Collect all services that need to run (including transitive dependencies)
        let mut needed = std::collections::HashSet::new();
        let mut stack = Vec::new();

        if let Some(selected) = selected_services {
            for s in selected {
                if !project.services.contains_key(s) {
                    return Err(DagError::ServiceNotFound(s.clone()));
                }
                stack.push(s.clone());
            }
        } else {
            for name in project.services.keys() {
                stack.push(name.clone());
            }
        }

        while let Some(current) = stack.pop() {
            if needed.insert(current.clone()) {
                if let Some(svc) = project.services.get(&current) {
                    for dep in &svc.depends_on {
                        if !needed.contains(dep) {
                            stack.push(dep.clone());
                        }
                    }
                }
            }
        }

        // Build directed graph: edge is dependency -> dependent (dep must run before dependent)
        let mut graph = DiGraph::<String, ()>::new();
        let mut node_indices: HashMap<String, NodeIndex> = HashMap::new();

        for name in &needed {
            let idx = graph.add_node(name.clone());
            node_indices.insert(name.clone(), idx);
        }

        for name in &needed {
            if let Some(svc) = project.services.get(name) {
                let target_idx = node_indices[name];
                for dep in &svc.depends_on {
                    if let Some(&dep_idx) = node_indices.get(dep) {
                        graph.add_edge(dep_idx, target_idx, ());
                    }
                }
            }
        }

        // Verify DAG using toposort
        if let Err(cycle) = toposort(&graph, None) {
            let cycle_node = &graph[cycle.node_id()];
            return Err(DagError::CircularDependency(format!(
                "service '{}' is part of a circular dependency",
                cycle_node
            )));
        }

        // Compute parallel layers (longest path from source)
        // Nodes with in-degree 0 are layer 0
        let mut layers: HashMap<String, usize> = HashMap::new();

        // Topologically sorted nodes
        let sorted = toposort(&graph, None).unwrap();
        for node_idx in sorted {
            let name = &graph[node_idx];
            let mut max_parent_layer = 0;
            let mut has_parents = false;

            for parent_idx in graph.neighbors_directed(node_idx, petgraph::Direction::Incoming) {
                let parent_name = &graph[parent_idx];
                let p_layer = layers.get(parent_name).copied().unwrap_or(0);
                if !has_parents || p_layer >= max_parent_layer {
                    max_parent_layer = p_layer;
                    has_parents = true;
                }
            }

            let node_layer = if has_parents {
                max_parent_layer + 1
            } else {
                0
            };
            layers.insert(name.clone(), node_layer);
        }

        let max_layer = layers.values().copied().max().unwrap_or(0);
        let mut batches = vec![Vec::new(); max_layer + 1];

        for (name, layer) in layers {
            batches[layer].push(name);
        }

        for batch in &mut batches {
            batch.sort();
        }

        Ok(Self { batches })
    }

    /// Batches of independent services to start in sequence (items in each batch run concurrently).
    pub fn execution_batches(&self) -> &[Vec<String>] {
        &self.batches
    }

    /// Batches to stop in reverse order.
    pub fn shutdown_batches(&self) -> Vec<Vec<String>> {
        let mut rev = self.batches.clone();
        rev.reverse();
        rev
    }
}

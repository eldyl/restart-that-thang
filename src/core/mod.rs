use anyhow::Context;
use chrono::{DateTime, Utc};
use petgraph::prelude::DiGraphMap;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

mod container;
mod docker;
mod label_parser;
mod schedule;

use container::{Container, ContainerHealth, ContainerName};
use docker::{
    docker_inspect_containers_health_and_start_time, docker_list_containers,
    docker_restart_container,
};
use label_parser::RttLabels;
use schedule::RestartSchedule;

#[derive(Debug, Error)]
pub enum ControllerError {
    #[error("No containers running on host machine")]
    NoContainersRunningOnHost,
    #[error("No containers with restart-that-thang labels running on host machine")]
    NoRTTLabelsDetected,
}

/// Deserialized JSON output from `docker ps` command.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
struct DockerPsOutput {
    #[serde(rename = "Names")]
    name: String,
    #[serde(rename = "Labels", default)]
    labels: String,
}

type RttCache = HashMap<ContainerName, Container>;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct State {
    pub containers: RttCache,
}


        if docker_output.is_empty() {
            anyhow::bail!(ControllerError::NoContainersRunningOnHost)
        }

        // Hold newly parsed containers with RTT labels
        let mut temp_containers: HashMap<ContainerName, Container> = HashMap::new();

        // Iterate through docker output string
        for line in docker_output.lines() {
            let docker_container: DockerPsOutput = match serde_json::from_str(line.trim()) {
                Ok(container) => container,
                Err(e) => {
                    log::error!("Failed to parse `docker ps` response from JSON: {e}");
                    continue;
                }
            };

            let name = &docker_container.name;

            let rtt_labels = match RttLabels::parse(&docker_container.labels) {
                Ok(labels) => {
                    let Some(labels) = labels else { continue };
                    labels
                }
                Err(e) => {
                    log::error!("Error while parsing labels for {name}: {e}");
                    continue;
                }
            };

            log::debug!(
                "{name} monitoring restarts of: {:?}",
                rtt_labels.watch_restarted
            );
            log::debug!(
                "{name} monitoring unhealthy state of: {:?}",
                rtt_labels.watch_unhealthy
            );
            log::debug!(
                "{name} restart interval time: {:?}",
                &rtt_labels.interval_time
            );
            log::debug!("{name} restart daily time: {:?}", &rtt_labels.daily_time);

            let schedule = RestartSchedule::new(rtt_labels.interval_time, rtt_labels.daily_time);

            // Determine containers schedule and next restart time
            let next_restart_time = if let Some(schedule) = schedule.as_ref() {
                if let Some(existing) = self.containers.get(name)
                    && existing.restart_schedule == Some(schedule.clone())
                {
                    existing.next_restart_time
                } else {
                    schedule.calc_next_restart_time()?
                }
            } else {
                None
            };

            log::debug!("{name} next restart time: {:?}", &next_restart_time);

            let container = Container::new(
                name,
                rtt_labels.watch_restarted,
                rtt_labels.watch_unhealthy,
                schedule,
                next_restart_time,
            );

            temp_containers.insert(name.to_string(), container);
            self.container_names.push(name.to_string());
        }

        if temp_containers.is_empty() {
            anyhow::bail!(ControllerError::NoRTTLabelsDetected)
        }

        // Store containers in persistent hashmap
        self.containers = temp_containers;

        Ok(())
    }

    /// Use `docker inspect` to obtain container start time, and health state.
    pub(crate) async fn fetch_and_set_start_times(&mut self) -> anyhow::Result<()> {
        if self.containers.is_empty() {
            anyhow::bail!(ControllerError::NoRTTLabelsDetected)
        }

        let docker_output =
            docker_inspect_containers_health_and_start_time(&self.container_names).await?;

        docker_output
            .lines()
            .filter(|line| !line.is_empty())
            .try_for_each(|line| {
                let line = line.trim_start_matches("/");
                let parts = line.split_whitespace().collect::<Vec<&str>>();

                // Set start time and health state for persisted containers
                match parts.as_slice() {
                    [name, timestamp_str, health_state, ..] => {
                        // Get the container that will have data included from docker inspect from
                        // the containers that are presently being tracked
                        let container_to_update =
                            self.containers.get_mut(*name).with_context(|| {
                                format!("Failed to get container {name} from hashmap")
                            })?;
                        let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
                            .with_context(|| format!("Failed to parse timestamp for {name}"))?
                            .with_timezone(&Utc);
                        // Update the fields that were parsed from docker inspect for the container
                        // we are tracking
                        container_to_update.set_health(health_state);
                        container_to_update.set_start_time(timestamp);

                        log::debug!("{name} health state: {health_state}");
                        log::debug!("{name} start time: {timestamp}");
                        Ok(())
                    }
                    _ => anyhow::bail!("Incorrect string format from inspect result"),
                }
            })?;

        Ok(())
    }

    /// Sort monitored containers based on their restart based dependencies.
    pub(crate) fn sort(&mut self) -> anyhow::Result<()> {
        // Thanks for the dependency based sorting strategy, Wez!
        // https://github.com/wez/docker-stack-deploy/blob/6f413fafe4609ad78873b2b4898f5a0b23b9a8f4/src/deploy_file.rs#L87
        let mut graph: DiGraphMap<&ContainerName, ()> = DiGraphMap::new();
        for (name, container) in self.containers.iter() {
            graph.add_node(name);
            for dep in &container.depends_on {
                if self.containers.contains_key(dep) {
                    graph.add_edge(dep, name, ());
                } else {
                    log::warn!("Container {name} depends on {dep}, but {dep} was not found with proper label")
                }
            }
        }

        let sorted_graph = petgraph::algo::toposort(&graph, None)
            .map_err(|e| anyhow::anyhow!("Circular dependency detected at {}", e.node_id()))?;

        for name in sorted_graph {
            if let Some(container) = self.containers.get(name).cloned() {
                self.sorted_containers.push(container);
            }
        }

        Ok(())
    }

    /// Determine if a container should be restarted during the current cycle.
    pub(crate) async fn restart_if_needed(&mut self) -> anyhow::Result<()> {
        let mut restarted_this_cycle = HashSet::new();

        for container in &self.sorted_containers {
            if restarted_this_cycle.contains(&container.name) {
                continue;
            }

            let mut needs_restart = false;
            let mut scheduled_restart = false;

            if let Some(current_container) = self.containers.get(&container.name)
                && current_container.next_restart_time.is_some()
                && current_container.needs_scheduled_restart()
            {
                // Container needs to restart based on scheduled restart time
                needs_restart = true;
                scheduled_restart = true;
            }

            if !needs_restart && !container.depends_on_healthy.is_empty() {
                for dep_name in &container.depends_on_healthy {
                    if let Some(dep_container) = self.containers.get(dep_name)
                        && dep_container.health == HealthState::UnHealthy
                    {
                        // Container needs to restart because relevant container is unhealthy
                        log::debug!(
                            "{} needs {} to be healthy, restarting {}",
                            container.name,
                            dep_container.name,
                            dep_container.name
                        );
                        needs_restart = true;
                        break;
                    }
                }
            }

            if !needs_restart && !container.depends_on.is_empty() {
                for dep_name in &container.depends_on {
                    // A dependency for this container has restarted, we need to restart this
                    // container
                    if restarted_this_cycle.contains(dep_name) {
                        needs_restart = true;
                        log::debug!(
                            "{} needs restart because {} was restarted this cycle",
                            container.name,
                            dep_name
                        );
                        break;
                    }
                    // Compare start times for container and its dependencies
                    if let Some(dep_container) = self.containers.get(dep_name) {
                        match (dep_container.start_time, container.start_time) {
                            (Some(dep_start), Some(container_start)) => {
                                if dep_start > container_start {
                                    needs_restart = true;
                                    log::debug!(
                                        "{dep_name} has restarted, will restart {} ",
                                        container.name,
                                    );
                                    break;
                                }
                            }
                            _ => {
                                log::error!(
                                    "Missing times for {} and {}",
                                    dep_name,
                                    container.name
                                );
                            }
                        }
                    }
                }
            };

            if needs_restart {
                match container.restart().await {
                    Ok(()) => {
                        restarted_this_cycle.insert(container.name.clone());

                        // If we have a time based restart, set the next restart time for the
                        // container
                        if scheduled_restart
                            && let Some(container_mut) = self.containers.get_mut(&container.name)
                            && let Err(e) = container_mut.set_next_restart_time()
                        {
                            log::error!(
                                "Failed to update next restart time for {}: {}",
                                container_mut.name,
                                e
                            );
                        }
                    }
                    Err(e) => {
                        log::error!("{e}");
                    }
                }
            }
        }

        restarted_this_cycle.clear();

        Ok(())
    }

    /// Clean up resources that do not need to persist through cycles.
    pub(crate) fn cleanup_after_cycle(&mut self) {
        self.sorted_containers.clear();
        self.container_names.clear();
    }
}

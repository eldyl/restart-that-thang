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

/// Fetches containers with `docker ps`, parses containers with RTT labels, and creates a
/// hashmap RttCache.
pub(crate) async fn fetch_rtt_containers(
    app_state: &State,
) -> anyhow::Result<(Vec<ContainerName>, RttCache)> {
    // List docker containers in json format with docker-cli
    let docker_output = docker_list_containers().await?;

    if docker_output.is_empty() {
        anyhow::bail!(ControllerError::NoContainersRunningOnHost)
    }

    // Hold newly parsed containers with RTT labels
    let mut containers: RttCache = HashMap::new();
    let mut container_names: Vec<ContainerName> = vec![];

    // Iterate through docker output string
    for line in docker_output.lines() {
        let docker_container: DockerPsOutput = match serde_json::from_str(line.trim()) {
            Ok(container) => container,
            Err(e) => {
                log::error!("Failed to parse `docker ps` response from JSON: {e}");
                continue;
            }
        };

        let name = ContainerName::new(&docker_container.name);

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
            "{} restart interval time: {:?}",
            name,
            &rtt_labels.interval_time
        );
        log::debug!("{} restart daily time: {:?}", name, &rtt_labels.daily_time);

        let schedule = RestartSchedule::new(rtt_labels.interval_time, rtt_labels.daily_time);

        // Determine containers schedule and next restart time
        let next_restart_time = if let Some(schedule) = schedule.as_ref() {
            if let Some(existing) = app_state.containers.get(&name)
                && existing.restart_schedule == Some(schedule.clone())
            {
                existing.next_restart_time
            } else {
                schedule.calc_next_restart_time()?
            }
        } else {
            None
        };

        log::debug!("{} next restart time: {:?}", name, &next_restart_time);

        let container = Container::new(
            name.to_owned(),
            rtt_labels.watch_restarted,
            rtt_labels.watch_unhealthy,
            schedule,
            next_restart_time,
        );

        container_names.push(name.clone());
        containers.insert(name, container);
    }

    if containers.is_empty() {
        anyhow::bail!(ControllerError::NoRTTLabelsDetected)
    }

    Ok((container_names, containers))
}

/// Use `docker inspect` to obtain container start time, and health state.
pub(crate) async fn fetch_and_set_start_times(
    rtt_container_names: Vec<ContainerName>,
    mut rtt_containers: RttCache,
) -> anyhow::Result<RttCache> {
    if rtt_containers.is_empty() {
        anyhow::bail!(ControllerError::NoRTTLabelsDetected)
    }

    let docker_output =
        docker_inspect_containers_health_and_start_time(&rtt_container_names).await?;

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
                    let container_name = ContainerName::new(*name);
                    let container_to_update = rtt_containers
                        .get_mut(&container_name)
                        .with_context(|| format!("Failed to get container {name} from hashmap"))?;
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

    Ok(rtt_containers)
}

/// Sort monitored containers based on their restart based dependencies.
pub(crate) fn rtt_containers_topo_sort(
    containers: &RttCache,
) -> anyhow::Result<Vec<ContainerName>> {
    // Thanks for the dependency based sorting strategy, Wez!
    // https://github.com/wez/docker-stack-deploy/blob/6f413fafe4609ad78873b2b4898f5a0b23b9a8f4/src/deploy_file.rs#L87
    let mut graph: DiGraphMap<&ContainerName, ()> = DiGraphMap::new();
    for (name, container) in containers.iter() {
        graph.add_node(name);
        for dep in &container.depends_on {
            if containers.contains_key(dep) {
                graph.add_edge(dep, name, ());
            } else {
                log::warn!(
                    "Container {name} depends on {dep}, but {dep} was not found with proper label",
                )
            }
        }
    }

    petgraph::algo::toposort(&graph, None)
        .map(|sorted| sorted.into_iter().cloned().collect())
        .map_err(|e| anyhow::anyhow!("Circular dependency detected at {}", e.node_id()))
}

/// Determine if a container should be restarted during the current cycle.
pub(crate) async fn restart_if_needed(
    mut rtt_containers: RttCache,
    sorted_containers: &[ContainerName],
) -> anyhow::Result<RttCache> {
    let mut restarted_this_cycle = HashSet::new();

    for container_name in sorted_containers {
        if restarted_this_cycle.contains(container_name) {
            continue;
        }

        let mut needs_restart = false;
        let mut scheduled_restart = false;

        if let Some(current_container) = rtt_containers.get(container_name)
            && current_container.next_restart_time.is_some()
            && current_container.needs_scheduled_restart()
        {
            // Container needs to restart based on scheduled restart time
            needs_restart = true;
            scheduled_restart = true;
        }

        let container = &rtt_containers[container_name];
        if !needs_restart && !container.depends_on_healthy.is_empty() {
            for dep_name in &container.depends_on_healthy {
                if let Some(dep_container) = rtt_containers.get(dep_name)
                    && dep_container.health == ContainerHealth::UnHealthy
                {
                    // Container needs to restart because relevant container is unhealthy
                    log::debug!(
                        "{container_name} needs {dep_name} to be healthy, restarting {container_name}",
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
                        "{container_name} needs restart because {dep_name} was restarted this cycle"
                    );
                    break;
                }
                // Compare start times for container and its dependencies
                if let Some(dep_container) = rtt_containers.get(dep_name) {
                    match (dep_container.start_time, container.start_time) {
                        (Some(dep_start), Some(container_start)) => {
                            if dep_start > container_start {
                                needs_restart = true;
                                log::debug!(
                                    "{dep_name} has restarted, will restart {container_name} ",
                                );
                                break;
                            }
                        }
                        _ => {
                            log::error!("Missing times for {dep_name} and {container_name}");
                        }
                    }
                }
            }
        };

        if needs_restart {
            match docker_restart_container(container_name).await {
                Ok(()) => {
                    restarted_this_cycle.insert(container_name.clone());

                    // If we have a time based restart, set the next restart time for the
                    // container
                    if scheduled_restart
                        && let Some(container_mut) = rtt_containers.get_mut(container_name)
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

    Ok(rtt_containers)
}

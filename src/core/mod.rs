use anyhow::Context;
use chrono::{DateTime, Utc};
use petgraph::prelude::DiGraphMap;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use tokio::process::Command;

mod container;
use container::{Container, HealthState, RestartSchedule};
mod label_parser;
use label_parser::RttLabels;

type ContainerName = String; // Using type to make purpose explicit

/// Deserialized JSON output from `docker ps` command.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
struct DockerPsOutput {
    #[serde(rename = "Names")]
    name: String,
    #[serde(rename = "Labels", default)]
    labels: String,
}

/// Serves as controller for the program to be used to monitor labels and restart specific
/// containers when appropriate.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Controller {
    /// Hashmap persists through cycles to keep state of monitored containers
    containers: HashMap<ContainerName, Container>,

    /// Field gets used for the inspect command passed to the docker-cli. Creating this vector is
    /// more efficient in terms of time than creating a vector from the containers keys to pass to
    /// the docker-cli.
    container_names: Vec<ContainerName>,

    /// Containers that are sorted by their dependency position in the dependency graph.
    sorted_containers: Vec<Container>,
}

impl Controller {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fetches containers with `docker ps`, parses containers with RTT labels, and creates a
    /// hashmap HashMap<ContainerName, Container>.
    pub(crate) async fn fetch_rtt_containers(&mut self) -> anyhow::Result<()> {
        // List docker containers in json format with docker-cli
        let output = Command::new("docker")
            .args(["ps", "-a", "--format", "json"])
            .output()
            .await
            .context("Failed to call `docker ps -a --format json`")?;

        // Create string from stdout
        let docker_output = String::from_utf8(output.stdout)
            .context("Failed to create string from docker output")?;

        if docker_output.is_empty() {
            anyhow::bail!("No containers running on host");
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

            let Some(rtt_labels) = RttLabels::parse(&docker_container.labels)? else {
                continue;
            };
            let name = &docker_container.name;

            log::debug!(
                "{name} monitoring restarts of: {:?}",
                rtt_labels.watch_restarted
            );
            log::debug!(
                "{name} monitoring unhealthy state of: {:?}",
                rtt_labels.watch_unhealthy
            );
            log::debug!("{name} restart schedule: {:?}", &rtt_labels.schedule);

            // Determine containers schedule and next restart time
            let (schedule, next_restart_time) = match rtt_labels.schedule {
                // Container has no schedule
                RestartSchedule::None => (None, None),
                // Container has a schedule
                schedule => {
                    if let Some(existing) = self.containers.get(name) {
                        // Container has been tracked previously
                        if existing.restart_schedule == Some(schedule.clone()) {
                            // Schedule remains the same, no need for recalculation
                            (Some(schedule), existing.next_restart_time)
                        } else {
                            // Schedule changed, recalculate next start time
                            let next_restart_time = self.calc_next_restart_time(&schedule)?;
                            (Some(schedule), next_restart_time)
                        }
                    } else {
                        // Container has not be previously tracked
                        let next_restart_time = self.calc_next_restart_time(&schedule)?;
                        (Some(schedule), next_restart_time)
                    }
                }
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

        // Store containers in persistent hashmap
        self.containers = temp_containers;

        Ok(())
    }

    /// Calculates the next restart time for a container that has a schedule label.
    fn calc_next_restart_time(
        &self,
        schedule: &RestartSchedule,
    ) -> anyhow::Result<Option<DateTime<Utc>>> {
        schedule.calc_next_restart_time()
    }

    /// Use `docker inspect` to obtain container start time, and health state.
    pub(crate) async fn fetch_and_set_start_times(&mut self) -> anyhow::Result<()> {
        if self.containers.is_empty() {
            anyhow::bail!("No containers running on host")
        }

        let output = Command::new("docker")
            .arg("inspect")
            .arg("--format")
            .arg("{{.Name}} {{.State.StartedAt}} {{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}")
            .args(&self.container_names)
            .output()
            .await
            .context("Failed to inspect provided docker containers")?;

        let docker_output =
            String::from_utf8(output.stdout).context("Failed to parse stdout into string")?;

        docker_output
            .lines()
            .filter(|line| !line.is_empty())
            .try_for_each(|line| {
                let line = line.trim_start_matches("/");
                let parts = line.split_whitespace().collect::<Vec<&str>>();

                // Set start time and health state for persisted containers
                match parts.as_slice() {
                    [name, timestamp_str, health_state, ..] => {
                        let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
                            .with_context(|| format!("Failed to parse timestamp for {name}"))?
                            .with_timezone(&Utc);
                        let container_to_update =
                            self.containers.get_mut(*name).with_context(|| {
                                format!("Failed to get container {name} from hashmap")
                            })?;
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

                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
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

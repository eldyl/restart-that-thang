use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum DockerError {
    #[error("Failed to execute `docker ps`")]
    DockerPsFailed {
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to execute `docker inspect`")]
    DockerInspectFailed {
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to restart docker container `{container_name}` due to timeout")]
    DockerRestartTimeout {
        container_name: String,
        #[source]
        source: tokio::time::error::Elapsed,
    },

    #[error("Failed to restart docker container `{container_name}`")]
    DockerRestartFailed {
        container_name: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to restart docker container `{container_name}`: {stderr}")]
    DockerRestartExitCode {
        container_name: String,
        stderr: String,
    },

    #[error("Failed to parse docker output as UTF-8 string")]
    OutputInvalid(#[from] std::string::FromUtf8Error),
}

pub async fn docker_list_containers() -> Result<String, DockerError> {
    let output = Command::new("docker")
        .args(["ps", "--format", "json"])
        .output()
        .await
        .map_err(|source| DockerError::DockerPsFailed { source })?;

    let docker_output = String::from_utf8(output.stdout)?;

    Ok(docker_output)
}

pub async fn docker_inspect_containers_health_and_start_time(
    containers: &[String],
) -> Result<String, DockerError> {
    let output = Command::new("docker")
            .arg("inspect")
            .arg("--format")
            .arg("{{.Name}} {{.State.StartedAt}} {{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}")
            .args(containers)
            .output()
            .await
            .map_err(|source| DockerError::DockerInspectFailed { source })?;

    let docker_output = String::from_utf8(output.stdout)?;

    Ok(docker_output)
}

pub async fn docker_restart_container(container_name: &str) -> Result<(), DockerError> {
    const CONTAINER_RESTART_TIMEOUT: u64 = 15;
    log::info!("Restarting: {container_name}");

    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(CONTAINER_RESTART_TIMEOUT),
        tokio::process::Command::new("docker")
            .args(["restart", container_name])
            .output(),
    )
    .await
    .map_err(|source| DockerError::DockerRestartTimeout {
        container_name: container_name.into(),
        source,
    })?
    .map_err(|source| DockerError::DockerRestartFailed {
        container_name: container_name.into(),
        source,
    })?;

    if output.status.success() {
        log::info!("Success! ({container_name})");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DockerError::DockerRestartExitCode {
            container_name: container_name.into(),
            stderr: stderr.into(),
        });
    }

    Ok(())
}

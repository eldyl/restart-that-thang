# Restart-That-Thang

Restart That Thang (RTT) makes restarting docker containers easy.

Do you - 

- Have containers that should restart if another container has restarted?

- Want a container to restart if a container is unhealthy?

- Want a container to restart at a specific time of day, or time interval?

Set some labels, and let RTT handle the rest.

## Features

- **Dependency based restarts** - Automatically restart containers when their 
dependencies restart
- **Container health based restarts** - Restart containers when monitored containers
become unhealthy
- **Interval based restarts** - Schedule restarts at regular time intervals
- **Time of day based restarts** - Schedule restarts at specific time of day
- **Label based configuration** - Simple Docker label configuration

## Getting Started

### Basic Compose Setup

```yaml
---
services:
  restart-that-thang:
    image: ghcr.io/eldyl/restart-that-thang:latest
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    healthcheck:
      test: wget -nv --tries=1 --spider http://127.0.0.1:8080/health || exit 1
      interval: 30s
      timeout: 5s
      retries: 2
```

### Configurable Environmental Variables

- **HOST**
    - Default: 127.0.0.1
    - IP address for healthcheck endpoint to bind to
- **PORT**
    - Default: 8080
    - Port for healthcheck endpoint to bind to
- **POLLING_INTERVAL** (seconds)
    - Default: 30
    - How often the application checks containers and processes restart logic
- **STARTUP_DELAY** (seconds)
    - Default: 30
    - Initial delay before application starts its monitoring cycle
- **RUST_LOG**
    - Default: info
    - The level of logging to display. For more verbose logging, you can set
    `RUST_LOG=debug`

### Docker Labels

Labels are prefixed with either `restart-that-thang` or `rtt`.

- `rtt.enable=true` (REQUIRED)
    - Indicates that a container will be monitored by restart-that-thang
- `rtt.watch.restarts=` 
    - Indicates that this container must restart if any of the
    containers in this list have restarted more recently than the container with
    this label
        - Accepts single value or comma delimited list
        - Example: `rtt.watch.restarts=container1`, or 
        `rtt.watch.restarts=container1,container2`
- `rtt.watch.unhealthy=` 
    - Indicates that this container must restart if any of
    the containers in this list have become unhealthy
        - Accepts single value or comma delimited list
        - Example: `rtt.watch.unhealthy=container1`, or 
        `rtt.watch.unhealthy=container1,container2`
- `rtt.schedule.interval=` 
    - Indicates that this container should restart on a
    specified time interval
        - Possible label values include: `2d` (days), `3h` (hours), `45m` (minutes),
        `50s` (seconds)
        - Example: `rtt.schedule.interval=130m` indicates the labeled container must
        restart every 130 minutes
- `rtt.schedule.time=` 
    - Indicates that this container should restart at a 
    specified time of day
        - Value should be in format `HH:MM`
        - Example: `rtt.schedule.time=23:00` indicates container should restart at
        23:00 (11:00pm) every day

Alternatively, you can use the long form to set any labels.
- Example: `restart-that-thang.enable=true`

### Example Usage

Example of setup with three separate compose files.

```yaml
---
services:
  restart-that-thang:
    image: ghcr.io/eldyl/restart-that-thang:latest
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    healthcheck:
      test: wget -nv --tries=1 --spider http://127.0.0.1:8080/health || exit 1
      interval: 30s
      timeout: 5s
      retries: 2
```

```yaml
---
services:
  gluetun:
    image: ghcr.io/qdm12/gluetun:latest
    container_name: gluetun
    restart: always
    ... # Your gluetun config
    labels:
      - "rtt.enable=true"                                   # Container will be monitored by RTT
      - "rtt.schedule.interval=150m"                        # Restart every 150 minutes
      - "rtt.watch.unhealthy=invidious,invidious-companion" # Restart if invidious or invidious-companion become unhealthy
```

```yaml
---
services:
  invidious:
    image: quay.io/invidious/invidious:master
    container_name: invidious
    restart: unless-stopped
    network_mode: "container:gluetun"
    ... # Your invidious config
    labels:
      - "rtt.enable=true"            # Container will be monitored by RTT
      - "rtt.watch.restarts=gluetun" # Restart if gluetun restarts
      - "rtt.schedule.interval=1h"   # Restart every hour
    healthcheck:
      test: ["CMD", "sh", "/healthcheck.sh"]
      interval: 20s
      timeout: 5s
      retries: 2
    depends_on:
      invidious-db:
        condition: service_healthy
      companion:
        condition: service_healthy

  companion:
    image: quay.io/invidious/invidious-companion:latest
    container_name: invidious-companion
    restart: unless-stopped
    network_mode: "container:gluetun"
    ... # Your companion config
    labels:
      - "rtt.enable=true"            # Container will be monitored by RTT
      - "rtt.watch.restarts=gluetun" # Restart if gluetun restarts
```

## How It Works

RTT operates on a continuous polling cycle (every POLLING_INTERVAL seconds):

1. **Container Discovery** - Scans all running containers for RTT labels `rtt.enable=true`
2. **Dependency Resolution** - Builds a dependency graph based on `rtt.watch.restarts=`
labels and sorts containers to handle restart order properly
3. **Start Time & Health State Assessment** - Uses docker inspect to check container start times
and health states
4. **Restart Decision & Execution** - Determines which containers need restarting:
    - Scheduled restart: Container has reached its scheduled restart time
    - Dependency restart: A watched container has restarted more recently than the
    dependant container
    - Health based restart: A watched container has become unhealthy
    - Cascade restart: A dependency was restarted in the current cycle
5. **Cleanup** - Clears temporary state and waits for the next polling cycle

## Contributing

Contributions are welcome. Please open an issue for any changes you would like 
to see before submitting a pull request.

## TODO

- [ ] Create a state system that will persist through application restarts. The purpose is to track
the last time a container restarted and continue to utilize that timestamp when the application
restarts.
- [ ] Add tests

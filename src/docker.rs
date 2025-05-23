use anyhow::Context;
use bollard::Docker;
use bollard::query_parameters::{InspectContainerOptions, InspectNetworkOptions};
use bollard::secret::{ContainerInspectResponse, EndpointSettings};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::OnceLock;

static DOCKER_PROVIDER: OnceLock<Docker> = OnceLock::new();

pub struct Config {
    domains: Vec<String>,
}

pub struct Container {
    id: String,
    config: Config,
    address: SocketAddr,
}

pub struct Network {
    containers_by_domain: HashMap<String, Vec<Container>>,
    containers: Vec<Container>,
}

impl Network {
    pub async fn get_mine() -> anyhow::Result<Self> {
        let network_ids = get_mine_networks()
            .await
            .context("failed to get mine networks")?
            .into_iter()
            .filter_map(|n| n.network_id)
            .collect::<Vec<_>>();

        let docker = get_or_init_docker_provider()?;

        let mut container_names = HashSet::new();

        for id in network_ids {
            let network = docker
                .inspect_network(&id, None::<InspectNetworkOptions>)
                .await
                .with_context(|| format!("failed to get {id} network"))?;

            network
                .containers
                .unwrap_or_else(HashMap::new)
                .into_values()
                .filter_map(|c| c.name)
                .for_each(|name| {
                    container_names.insert(name);
                });
        }

        let mut containers = Vec::with_capacity(container_names.len());

        for name in container_names {
            containers.push(
                Container::from_name(name.as_str())
                    .await
                    .with_context(|| format!("failed to get container {name}")),
            );
        }

        unimplemented!()
    }
}

impl Container {
    pub async fn from_name(name: &str) -> anyhow::Result<Self> {
        let docker = get_or_init_docker_provider()?;

        let container = docker
            .inspect_container(name, None::<InspectContainerOptions>)
            .await
            .context("failed to inspect container")?;

        Self::from_inspect(container)
    }

    pub fn from_inspect(inspect: ContainerInspectResponse) -> anyhow::Result<Self> {
        let id = inspect.id.context("container does not has an id")?;
        let address = inspect
            .network_settings
            .context("container does not has any network")?
            .ip_address
            .context("container does not has any ip address")?;

        let address = SocketAddr::from_str(&address)
            .with_context(|| format!("failed to parse address: {}", address))?;

        let labels = inspect
            .config
            .context("container does not has config")?
            .labels
            .unwrap_or_else(HashMap::default);

        let config = Config::from_labels(labels).context("failed to parse labels as config")?;

        Ok(Self {
            id,
            config,
            address,
        })
    }

    pub async fn get_me() -> anyhow::Result<Self> {
        let hostname = std::env::var("HOSTNAME").context("missing hostname env var")?;
        let docker = get_or_init_docker_provider()?;

        let container = docker
            .inspect_container(&hostname, None::<InspectContainerOptions>)
            .await
            .context("failed to get container")?;

        let id = container.id.context("container does not has an id")?;
        let address = container
            .network_settings
            .context("container does not has any network")?
            .ip_address
            .context("container does not has any ip address")?;

        let address = SocketAddr::from_str(&address)
            .with_context(|| format!("failed to parse address: {}", address))?;

        let labels = container
            .config
            .context("container does not has config")?
            .labels
            .unwrap_or_else(HashMap::default);

        let config = Config::from_labels(labels).context("failed to parse labels as config")?;

        Ok(Self {
            id,
            config,
            address,
        })
    }
}

impl Config {
    pub fn from_labels(labels: HashMap<String, String>) -> anyhow::Result<Self> {
        let mut domains = Vec::new();
        let domain = labels.get("proxy.domain");

        if let Some(domain) = domain {
            domains.push(domain.to_owned());
        }

        Ok(Self { domains })
    }
}

fn get_or_init_docker_provider<'a>() -> anyhow::Result<&'a Docker> {
    if let Some(instance) = DOCKER_PROVIDER.get() {
        return Ok(instance);
    }

    let instance =
        Docker::connect_with_socket_defaults().context("failed to connect docker socket")?;

    DOCKER_PROVIDER.set(instance);

    Ok(DOCKER_PROVIDER.get().unwrap())
}

async fn get_mine_networks() -> anyhow::Result<Vec<EndpointSettings>> {
    let hostname = std::env::var("HOSTNAME").context("missing hostname env var")?;
    let docker = get_or_init_docker_provider()?;

    let container = docker
        .inspect_container(&hostname, None::<InspectContainerOptions>)
        .await
        .context("failed to get container")?;

    let networks = container
        .network_settings
        .context("container does not have network settings")?
        .networks
        .unwrap_or_else(HashMap::new)
        .into_values()
        .collect::<Vec<_>>();

    Ok(networks)
}

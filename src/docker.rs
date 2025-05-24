use anyhow::Context;
use bollard::Docker;
use bollard::query_parameters::{InspectContainerOptions, InspectNetworkOptions};
use bollard::secret::{ContainerInspectResponse, EndpointSettings};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::OnceLock;

static DOCKER_PROVIDER: OnceLock<Docker> = OnceLock::new();

#[derive(Debug, Default, Clone)]
pub struct Config {
    domains: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Container {
    id: String,
    config: Config,
    address: IpAddr,
}

#[derive(Debug)]
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

        let mut containers = Vec::new();

        for id in network_ids {
            let network = docker
                .inspect_network(&id, None::<InspectNetworkOptions>)
                .await
                .with_context(|| format!("failed to get {id} network"))?;

            containers = network
                .containers
                .unwrap_or_else(HashMap::new)
                .into_values()
                .filter_map(|c| c.name.zip(c.ipv4_address))
                .map(|(n, a)| Container::new(n, &a))
                .collect::<anyhow::Result<HashSet<_>>>()
                .with_context(|| format!("failed to parse network {id:?}"))?
                .into_iter()
                .collect::<Vec<_>>();
        }

        let mut containers_by_domain = HashMap::<String, Vec<_>>::new();

        for container in containers.iter_mut() {
            container.load_config().await.with_context(|| {
                format!("failed to load config for container {:?}", container.id)
            })?;

            let domain = match container.get_config().domains().iter().next() {
                Some(domain) => domain.as_str(),
                None => continue,
            };

            if let Some(entry) = containers_by_domain.get_mut(domain) {
                entry.push(container.clone());
            } else {
                containers_by_domain.insert(domain.to_owned(), vec![container.clone()]);
            }
        }

        Ok(Self {
            containers_by_domain,
            containers,
        })
    }
}

impl Container {
    pub fn new(id: String, addr: &str) -> anyhow::Result<Self> {
        let addr = addr.chars().take_while(|c| *c != '/').collect::<String>();

        let address = IpAddr::from_str(&addr)
            .with_context(|| format!("failed to parse address {:?}", addr))?;

        Ok(Self {
            id,
            config: Config::default(),
            address,
        })
    }

    pub async fn load_config(&mut self) -> anyhow::Result<()> {
        let docker = get_or_init_docker_provider()?;

        let inspect = docker
            .inspect_container(&self.id, None::<InspectContainerOptions>)
            .await
            .context("failed to inspect container")?;

        let labels = inspect
            .config
            .context("container does not has config")?
            .labels
            .unwrap_or_else(HashMap::default);

        self.config = Config::from_labels(labels).context("failed to parse labels as config")?;

        Ok(())
    }

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

        let address = address
            .chars()
            .take_while(|c| *c == '/')
            .collect::<String>();

        let address = IpAddr::from_str(&address)
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

    pub fn get_config(&self) -> &Config {
        &self.config
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

        let address = IpAddr::from_str(&address)
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

    pub fn domains(&self) -> &[String] {
        self.domains.as_slice()
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

impl std::hash::Hash for Container {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(self.id.as_bytes());
    }
}

impl PartialEq for Container {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl Eq for Container {}

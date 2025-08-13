use anyhow::Context;
use bollard::Docker;
use bollard::query_parameters::InspectContainerOptions;
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;

pub struct Container {
    id: String,
    ip_addr: IpAddr,
    config: Option<Config>,
}

pub struct Config {
    port: Option<u16>,
    domains: Vec<String>,
}

impl Container {
    pub fn new(id: String, ip_addr: &str) -> anyhow::Result<Self> {
        let ip_addr = match ip_addr.find('/') {
            Some(pos) => &ip_addr[..pos],
            None => ip_addr,
        };

        let ip_addr = IpAddr::from_str(ip_addr)
            .with_context(|| format!("failed to parse {ip_addr} as ip addr"))?;

        Ok(Self {
            id,
            ip_addr,
            config: None,
        })
    }

    pub fn get_ip_addr(&self) -> IpAddr {
        self.ip_addr
    }

    pub fn get_domains_unchecked(&self) -> &[String] {
        &self
            .config
            .as_ref()
            .expect("missing expected container config")
            .domains
    }

    pub fn get_port(&self) -> Option<u16> {
        self.config.as_ref().and_then(|c| c.port)
    }

    pub async fn load_config(&mut self, client: &Docker) -> anyhow::Result<bool> {
        let inspect = client
            .inspect_container(&self.id, None::<InspectContainerOptions>)
            .await
            .context("failed to inspect container")?;

        let labels = inspect
            .config
            .context("container does not has config")?
            .labels
            .unwrap_or_else(HashMap::default);

        let config = Config::from_labels(labels).context("failed to parse config")?;
        let is_loaded = config.is_some();

        self.config = config;

        Ok(is_loaded)
    }
}

impl Config {
    pub fn from_labels(labels: HashMap<String, String>) -> anyhow::Result<Option<Self>> {
        let domain = match labels.get("proxy.domain") {
            Some(domain) => domain.trim().to_owned(),
            None => return Ok(None),
        };

        let port = match labels.get("proxy.port") {
            Some(port) => Some(
                u16::from_str(port)
                    .with_context(|| format!("failed to parse port {port} as u16"))?,
            ),
            None => None,
        };

        Ok(Some(Self {
            domains: vec![domain],
            port,
        }))
    }
}

impl Eq for Container {}

impl PartialEq for Container {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl PartialOrd for Container {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Container {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

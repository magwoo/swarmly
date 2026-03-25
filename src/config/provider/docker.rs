use anyhow::Context;
use bollard::Docker;
use bollard::query_parameters::{InspectContainerOptions, InspectNetworkOptions, ListServicesOptions};
use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use self::container::Container;
use super::{ConfigProvider, Value};

mod container;

type AsyncCallback =
    dyn Fn(&Value) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static;

#[derive(Clone)]
pub struct DockerConfig {
    client: Docker,
    callbacks: Arc<RwLock<Vec<Box<AsyncCallback>>>>,
}

impl DockerConfig {
    pub fn new() -> anyhow::Result<Self> {
        let client =
            Docker::connect_with_socket_defaults().context("failed to connect to docker")?;
        let callbacks = Arc::new(RwLock::new(Vec::default()));

        Ok(Self { client, callbacks })
    }

    async fn get_current_networks(&self) -> anyhow::Result<Vec<String>> {
        let hostname = std::env::var("HOSTNAME").context("missing hostname env var")?;

        let container = self
            .client
            .inspect_container(&hostname, None::<InspectContainerOptions>)
            .await
            .context("failed to get container")?;

        let ids = container
            .network_settings
            .context("container does not have network settings")?
            .networks
            .unwrap_or_default()
            .into_values()
            .filter_map(|e| e.network_id)
            .collect();

        Ok(ids)
    }

    async fn try_swarm_update(&self) -> anyhow::Result<Value> {
        let network_ids = self.get_current_networks().await?;

        let services = self
            .client
            .list_services(None::<ListServicesOptions>)
            .await
            .context("failed to list swarm services")?;

        let mut result: HashMap<String, Vec<SocketAddr>> = HashMap::new();

        for service in services {
            let labels = service
                .spec
                .as_ref()
                .and_then(|s| s.labels.as_ref());

            let labels = match labels {
                Some(l) => l,
                None => continue,
            };

            let domain = match labels.get("proxy.domain") {
                Some(d) => d.trim().to_owned(),
                None => continue,
            };

            let port: u16 = labels
                .get("proxy.port")
                .and_then(|p| p.trim().parse().ok())
                .unwrap_or(80);

            let vips = service
                .endpoint
                .as_ref()
                .and_then(|e| e.virtual_ips.as_ref());

            let addrs: Vec<SocketAddr> = vips
                .into_iter()
                .flatten()
                .filter(|vip| {
                    vip.network_id
                        .as_ref()
                        .map(|id| network_ids.contains(id))
                        .unwrap_or(false)
                })
                .filter_map(|vip| {
                    vip.addr.as_ref().and_then(|a| parse_vip_ip(a))
                })
                .map(|ip| SocketAddr::new(ip, port))
                .collect();

            if !addrs.is_empty() {
                result.entry(domain).or_default().extend(addrs);
            }
        }

        Ok(result.into_iter().collect())
    }

    async fn try_container_update(&self) -> anyhow::Result<Value> {
        let network_ids = self.get_current_networks().await?;
        let containers = self.get_containers_in_networks(&network_ids).await?;

        let mut result: HashMap<String, Vec<SocketAddr>> = HashMap::new();

        containers.iter().for_each(|c| {
            let port = c.get_port().unwrap_or(80);
            let addr = SocketAddr::new(c.get_ip_addr(), port);

            c.get_domains_unchecked().iter().for_each(|d| {
                result.entry(d.to_owned()).or_default().push(addr);
            });
        });

        Ok(result.into_iter().collect())
    }

    async fn get_containers_in_networks(
        &self,
        network_ids: &[String],
    ) -> anyhow::Result<BTreeSet<Container>> {
        let mut unfiltered_containers = BTreeSet::new();

        for id in network_ids {
            let network = self
                .client
                .inspect_network(id, None::<InspectNetworkOptions>)
                .await
                .with_context(|| format!("failed to get {id} network"))?;

            let containers = network
                .containers
                .unwrap_or_default()
                .into_iter()
                .filter(|(id, _)| id.len() == 64)
                .filter_map(|(id, c)| c.ipv4_address.map(|a| (id, a)))
                .map(|(id, ipv4)| Container::new(id, &ipv4))
                .collect::<anyhow::Result<Vec<_>>>()
                .context("failed to check network containers")?;

            unfiltered_containers.extend(containers);
        }

        let mut filtered_containers = BTreeSet::new();

        for mut container in unfiltered_containers {
            if container
                .load_config(&self.client)
                .await
                .context("failed to load container config")?
            {
                filtered_containers.insert(container);
            }
        }

        Ok(filtered_containers)
    }
}

fn parse_vip_ip(addr: &str) -> Option<IpAddr> {
    let ip_str = addr.split('/').next()?;
    IpAddr::from_str(ip_str).ok()
}

impl ConfigProvider for DockerConfig {
    fn set_update_callback<F, Fut>(&self, callback: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let boxed = Box::new(move |value: &Value| {
            Box::pin(callback(value.clone())) as Pin<Box<dyn Future<Output = ()> + Send>>
        });

        self.callbacks.write().unwrap().push(boxed);
    }

    async fn update(&self) -> anyhow::Result<Value> {
        let value = match self.try_swarm_update().await {
            Ok(v) => {
                tracing::debug!("using docker swarm service discovery");
                v
            }
            Err(err) => {
                tracing::debug!("swarm unavailable ({}), falling back to container mode", err);
                self.try_container_update().await?
            }
        };

        let futures: Vec<_> = self.callbacks.read().unwrap().iter()
            .map(|cb| cb(&value))
            .collect();

        for fut in futures {
            fut.await;
        }

        Ok(value)
    }
}

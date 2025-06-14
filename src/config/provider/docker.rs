use anyhow::Context;
use bollard::Docker;
use bollard::query_parameters::{InspectContainerOptions, InspectNetworkOptions};
use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

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
            .unwrap_or_else(HashMap::default)
            .into_values()
            .filter_map(|e| e.network_id)
            .collect::<Vec<_>>();

        Ok(ids)
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
                .unwrap_or_else(HashMap::default)
                .into_iter()
                .filter(|(id, _)| id.len() == 64)
                .filter_map(|(id, c)| c.ipv4_address.map(|a| (id, a)))
                .map(|(id, ipv4)| Container::new(id, &ipv4))
                .collect::<anyhow::Result<Vec<_>>>()
                .context("failed to check network containers")?;

            unfiltered_containers.extend(containers.into_iter());
        }

        let mut filtered_containers = BTreeSet::new();

        for mut container in unfiltered_containers {
            if !container
                .load_config(&self.client)
                .await
                .context("failed to load container config")?
            {
                continue;
            }

            filtered_containers.insert(container);
        }

        Ok(filtered_containers)
    }
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

        self.callbacks.blocking_write().push(boxed);
    }

    async fn update(&self) -> anyhow::Result<Value> {
        let network_ids = self
            .get_current_networks()
            .await
            .context("failed to get current network ids")?;

        let containers = self
            .get_containers_in_networks(&network_ids)
            .await
            .context("failed to get containers from network ids")?;

        let mut result = HashMap::new();

        containers.iter().for_each(|c| {
            let port = c.get_port().unwrap_or(80);
            let addr = SocketAddr::new(c.get_ip_addr(), port);

            c.get_domains_unchecked().iter().for_each(|d| {
                result.entry(d.to_owned()).or_insert(Vec::new()).push(addr);
            });
        });

        let value = result.into_iter().collect();

        for callback in self.callbacks.read().await.iter() {
            callback(&value).await;
        }

        Ok(value)
    }
}

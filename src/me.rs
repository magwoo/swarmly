use bollard::query_parameters::{InspectContainerOptions, InspectNetworkOptions};
use pingora::server::ShutdownWatch;
use pingora::services::background::BackgroundService;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct MeBackground(pub Arc<RwLock<HashMap<String, String>>>);

#[async_trait::async_trait]
impl BackgroundService for MeBackground {
    async fn start(&self, _shutdown: ShutdownWatch) {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let hostname = std::env::var("HOSTNAME").unwrap();
            let docker = bollard::Docker::connect_with_socket_defaults().unwrap();

            let me = docker
                .inspect_container(&hostname, None::<InspectContainerOptions>)
                .await
                .unwrap();

            let networks = me.network_settings.clone().unwrap().networks.unwrap();

            let mut upstreams = HashMap::new();

            for endpoint in networks.values() {
                let network_id = match endpoint.network_id.as_ref() {
                    Some(id) => id,
                    None => continue,
                };

                let network = docker
                    .inspect_network(network_id, None::<InspectNetworkOptions>)
                    .await
                    .unwrap();

                if let Some(containers) = network.containers {
                    for (id, network_container) in containers {
                        let container = docker
                            .inspect_container(&id, None::<InspectContainerOptions>)
                            .await
                            .unwrap();

                        let labels = container.config.and_then(|c| c.labels).map(|l| {
                            l.into_iter()
                                .filter(|(k, _)| k.starts_with("proxy."))
                                .collect::<HashMap<_, _>>()
                        });

                        if let (Some(labels), Some(addr)) = (labels, network_container.ipv4_address)
                        {
                            let domain = match labels.get("proxy.domain") {
                                Some(domain) => domain,
                                None => continue,
                            };

                            upstreams.insert(
                                domain.to_owned(),
                                format!("{}:8080", addr.split('/').next().unwrap()),
                            );
                        }
                    }
                }
            }
            println!("upstreams: {:#?}", upstreams);

            *self.0.write().await = upstreams;
        }
    }
}

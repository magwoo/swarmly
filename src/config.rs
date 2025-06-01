use pingora::server::ShutdownWatch;
use pingora::services::background::BackgroundService;
use std::time::Duration;

use self::provider::ConfigProvider;
use crate::proxy::Gateway;

pub mod provider;

pub struct ConfigRefresher<P> {
    provider: P,
    gateway: Gateway,
}

impl<P: ConfigProvider> ConfigRefresher<P> {
    pub fn new(provider: P, gateway: Gateway) -> Self {
        Self { provider, gateway }
    }
}

#[async_trait::async_trait]
impl<P: ConfigProvider + Send + Sync> BackgroundService for ConfigRefresher<P> {
    async fn start(&self, shutdown: ShutdownWatch) {
        tokio::time::sleep(Duration::from_secs(1)).await;

        loop {
            if shutdown.borrow().has_changed() {
                break;
            }

            let upstreams = match self.provider.update().await {
                Ok(upstreams) => upstreams,
                Err(err) => {
                    println!("failed to update config provider: {err:?}");
                    continue;
                }
            };

            println!("config updated: {:#?}", upstreams);

            self.gateway.update(upstreams).await;

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }
}

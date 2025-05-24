use pingora::server::ShutdownWatch;
use pingora::services::background::BackgroundService;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::docker::Network;

pub struct MeBackground(pub Arc<RwLock<Network>>);

#[async_trait::async_trait]
impl BackgroundService for MeBackground {
    async fn start(&self, _shutdown: ShutdownWatch) {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            let network = Network::get_mine().await.unwrap();

            let mut upstreams = self.0.write().await;

            *upstreams = network;
        }
    }
}

use pingora::lb::discovery::ServiceDiscovery;
use pingora::lb::selection::RoundRobin;
use pingora::lb::{Backend, Backends, LoadBalancer};
use pingora::server::ShutdownWatch;
use pingora::services::background::BackgroundService;
use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpSocket;
use tokio::sync::RwLock;

use crate::docker::{Container, Network};

pub struct MeBackground(pub Arc<RwLock<HashMap<String, LoadBalancer<RoundRobin>>>>);

pub struct PingDiscovery(Vec<SocketAddr>);

#[async_trait::async_trait]
impl BackgroundService for MeBackground {
    async fn start(&self, _shutdown: ShutdownWatch) {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let network = Network::get_mine().await.unwrap();

            let mut upstreams = HashMap::default();

            for (domain, containers) in network.get_containers_by_domain() {
                let backends = Backends::new(Box::new(PingDiscovery::new(containers)));
                let mut load_balancer = LoadBalancer::from_backends(backends);
                load_balancer.update().await.unwrap();
                load_balancer.update_frequency = Some(std::time::Duration::from_secs(5));

                upstreams.insert(domain, load_balancer);
            }

            *self.0.write().await = upstreams;

            tokio::time::sleep(Duration::from_secs(9)).await;
        }
    }
}

impl PingDiscovery {
    pub fn new(containers: Vec<Container>) -> Self {
        let addrs = containers
            .into_iter()
            .map(|c| c.get_addr())
            .collect::<Vec<_>>();

        Self(addrs)
    }
}

#[async_trait::async_trait]
impl ServiceDiscovery for PingDiscovery {
    async fn discover(&self) -> pingora::Result<(BTreeSet<Backend>, HashMap<u64, bool>)> {
        let mut timings = Vec::new();

        for addr in self.0.iter() {
            let socket = TcpSocket::new_v4().unwrap();
            let now = std::time::Instant::now();
            if socket.connect(*addr).await.is_err() {
                eprintln!("failed to ping: {:?}", addr);
                continue;
            }
            timings.push((addr, now.elapsed()));
        }

        println!("discover called: {:?}", timings);

        timings.sort_by(|(_, a), (_, b)| b.cmp(a));
        timings.truncate(1);

        let backends = timings
            .into_iter()
            .map(|(a, _)| Backend::new(a.to_string().as_str()).unwrap())
            .collect::<BTreeSet<_>>();

        Ok((backends, HashMap::default()))
    }
}

use pingora::Result;
use pingora::lb::Backend;
use pingora::lb::discovery::ServiceDiscovery;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::TcpSocket;

pub struct PingDiscovery {
    upstreams: Vec<SocketAddr>,
}

impl PingDiscovery {
    pub fn new(upstreams: Vec<SocketAddr>) -> Self {
        Self { upstreams }
    }
}

#[async_trait::async_trait]
impl ServiceDiscovery for PingDiscovery {
    async fn discover(&self) -> Result<(BTreeSet<Backend>, HashMap<u64, bool>)> {
        let mut timings = Vec::new();

        for upstream in self.upstreams.iter() {
            let socket = match TcpSocket::new_v4() {
                Ok(socket) => socket,
                Err(err) => {
                    println!("failed to create tcp socket: {err:?}");
                    continue;
                }
            };

            let start = std::time::Instant::now();
            match socket.connect(*upstream).await {
                Ok(_) => (),
                Err(_) => continue,
            };

            let elapsed = start.elapsed();

            timings.push((*upstream, elapsed));
        }

        timings.sort_by(|(_, a), (_, b)| a.cmp(b));

        println!("discovery results: {:?}", timings);

        timings.truncate(1);

        let upstreams = timings
            .into_iter()
            .map(|(a, _)| a)
            .map(|a| Backend::new(a.to_string().as_str()).expect("addr must be valid"));

        Ok((BTreeSet::from_iter(upstreams), HashMap::default()))
    }
}

use pingora::prelude::*;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::docker::Network;

#[derive(Debug)]
pub struct Gateway(pub Arc<RwLock<Network>>);

#[async_trait::async_trait]
impl ProxyHttp for Gateway {
    type CTX = Option<HttpPeer>;

    fn new_ctx(&self) -> Self::CTX {
        None
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool>
    where
        Self::CTX: Send + Sync,
    {
        let host = match session.get_header("Host").and_then(|h| h.to_str().ok()) {
            Some(host) => host,
            None => {
                session.respond_error(404).await.unwrap();
                return Ok(true);
            }
        };

        let upstreams = self.0.read().await;
        let ip = match upstreams.search(host) {
            Some(upstream) if !upstream.is_empty() => upstream[0],
            _ => {
                session.respond_error(404).await.unwrap();
                return Ok(true);
            }
        };

        let upstream = SocketAddr::new(ip, 80);

        *ctx = Some(HttpPeer::new(upstream, false, String::new()));

        Ok(false)
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let upstream = ctx.clone().unwrap();
        let client_addr = session
            .downstream_session
            .client_addr()
            .expect("missing client addr");

        println!("{:?} -> {}", client_addr, upstream);

        Ok(Box::new(upstream))
    }
}

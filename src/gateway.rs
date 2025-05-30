use pingora::lb::selection::RoundRobin;
use pingora::prelude::*;
use pingora::protocols::l4::socket::SocketAddr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Gateway(pub Arc<RwLock<HashMap<String, LoadBalancer<RoundRobin>>>>);

#[async_trait::async_trait]
impl ProxyHttp for Gateway {
    type CTX = Option<SocketAddr>;

    fn new_ctx(&self) -> Self::CTX {
        None
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool>
    where
        Self::CTX: Send + Sync,
    {
        let domain = match session.get_header("Host").and_then(|h| h.to_str().ok()) {
            Some(domain) => domain,
            None => {
                session.respond_error(404).await?;
                return Ok(true);
            }
        };

        let upstreams = self.0.read().await;
        let backend = match upstreams.get(domain) {
            Some(lb) => match lb.select(b"", 9) {
                Some(backend) => backend,
                None => {
                    session.respond_error(502).await?;
                    return Ok(true);
                }
            },
            _ => {
                session.respond_error(404).await?;
                return Ok(true);
            }
        };

        *ctx = Some(backend.addr);

        Ok(false)
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let upstream = ctx.as_ref().unwrap();
        let client_addr = session
            .downstream_session
            .client_addr()
            .expect("missing client addr");

        println!("{:?} -> {}", client_addr, upstream);

        Ok(Box::new(HttpPeer::new(upstream, false, String::new())))
    }
}

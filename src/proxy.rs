use std::str::FromStr;

use pingora::Result;
use pingora::prelude::*;
use pingora::protocols::l4::socket::SocketAddr;
use pingora::proxy::{ProxyHttp, Session};

pub use self::gateway::Gateway;

mod discovery;
mod gateway;

pub struct SwarmProxy {
    gateway: Gateway,
}

impl SwarmProxy {
    pub fn new(gateway: Gateway) -> Self {
        Self { gateway }
    }
}

#[async_trait::async_trait]
impl ProxyHttp for SwarmProxy {
    type CTX = Option<SocketAddr>;

    fn new_ctx(&self) -> Self::CTX {
        None
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool>
    where
        Self::CTX: Send + Sync,
    {
        if session.req_header().uri.path().starts_with("/.well-known") {
            *ctx = Some(SocketAddr::Inet(
                std::net::SocketAddr::from_str("127.0.0.1:7765").expect("addr must be valid"),
            ));
            return Ok(false);
        }

        let domain = session
            .get_header("host")
            .and_then(|h| h.to_str().ok())
            .or_else(|| session.req_header().uri.host());

        let domain = match domain {
            Some(host) => host.trim(),
            None => {
                session.respond_error(400).await?;
                return Ok(true);
            }
        };

        let upstream = match self.gateway.process(domain).await {
            Some(backend) => backend.addr,
            None => {
                session.respond_error(404).await?;
                return Ok(true);
            }
        };

        *ctx = Some(upstream);

        Ok(false)
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let upstream = ctx.as_ref().expect("upstream must be selected");
        let uri = &session.req_header().uri;

        println!("{} -> {}", uri, upstream);

        Ok(Box::new(HttpPeer::new(upstream, false, String::default())))
    }
}

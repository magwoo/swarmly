use std::str::FromStr;
use std::time::Instant;

use bytes::Bytes;
use pingora::Result;
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::protocols::l4::socket::SocketAddr;
use pingora::proxy::{ProxyHttp, Session};

pub use self::gateway::Gateway;

mod discovery;
mod gateway;

pub struct ProxyCtx {
    upstream: Option<SocketAddr>,
    start: Instant,
}

pub struct SwarmProxy {
    gateway: Gateway,
    tls_enabled: bool,
}

impl SwarmProxy {
    pub fn new(gateway: Gateway, tls_enabled: bool) -> Self {
        Self {
            gateway,
            tls_enabled,
        }
    }
}

#[async_trait::async_trait]
impl ProxyHttp for SwarmProxy {
    type CTX = ProxyCtx;

    fn new_ctx(&self) -> Self::CTX {
        ProxyCtx {
            upstream: None,
            start: Instant::now(),
        }
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool>
    where
        Self::CTX: Send + Sync,
    {
        let path = session.req_header().uri.path();

        // Forward ACME http-01 challenges to the local challenge service.
        if path.starts_with("/.well-known/acme-challenge/") {
            ctx.upstream = Some(SocketAddr::Inet(
                std::net::SocketAddr::from_str("127.0.0.1:7765").expect("addr must be valid"),
            ));
            return Ok(false);
        }

        // Health check — always respond immediately.
        if path == "/health" || path == "/healthz" {
            let mut header = ResponseHeader::build(200, None)?;
            header.insert_header("content-type", "text/plain")?;
            header.insert_header("content-length", "2")?;
            session
                .write_response_header(Box::new(header), false)
                .await?;
            session
                .write_response_body(Some(Bytes::from_static(b"ok")), true)
                .await?;
            return Ok(true);
        }

        // HTTP → HTTPS redirect when TLS is enabled and the connection is plain HTTP.
        if self.tls_enabled {
            let is_tls = session
                .server_addr()
                .and_then(|a| a.as_inet())
                .map(|a| a.port() == 443)
                .unwrap_or(false);

            if !is_tls {
                let host = session
                    .get_header("host")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("");
                let uri = session.req_header().uri.clone();
                let location = format!(
                    "https://{}{}",
                    host,
                    uri.path_and_query().map(|p| p.as_str()).unwrap_or("/")
                );
                let mut header = ResponseHeader::build(301, None)?;
                header.insert_header("location", location)?;
                header.insert_header("content-length", "0")?;
                session
                    .write_response_header(Box::new(header), true)
                    .await?;
                return Ok(true);
            }
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

        ctx.upstream = Some(upstream);

        Ok(false)
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let upstream = ctx.upstream.as_ref().expect("upstream must be selected");
        Ok(Box::new(HttpPeer::new(upstream, false, String::default())))
    }

    async fn logging(&self, session: &mut Session, _e: Option<&pingora::Error>, ctx: &mut Self::CTX) {
        let req = session.req_header();
        let method = req.method.as_str();
        let host = req
            .headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("-");
        let path = req.uri.path();
        let status = session
            .response_written()
            .map(|r| r.status.as_u16())
            .unwrap_or(0);
        let latency_ms = ctx.start.elapsed().as_millis();
        let client = session
            .client_addr()
            .and_then(|a| a.as_inet())
            .map(|a| a.ip().to_string())
            .unwrap_or_else(|| "-".to_owned());

        tracing::info!(
            "{} \"{} {} {}\" {} {}ms",
            client, method, host, path, status, latency_ms
        );
    }
}

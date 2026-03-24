use pingora::prelude::*;
use pingora::services::listening::Service;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use self::config::ConfigRefresher;
use self::config::provider::docker::DockerConfig;
use self::proxy::Gateway;
use self::proxy::SwarmProxy;
use self::tls::AcmeChallengeService;
use self::tls::TlsResolver;

mod config;
mod proxy;
mod redis;
mod tls;

fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let mut server = Server::new(None).unwrap();

    let gateway = Gateway::default();
    let config_provider = DockerConfig::new().unwrap();

    let (redis, tls_resolver) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime for init")
        .block_on(async {
            let redis = redis::RedisClient::from_env()
                .await
                .expect("failed to connect to redis");

            if redis.is_some() {
                tracing::info!("redis connected — using distributed mode");
            } else {
                tracing::info!("no REDIS_URL set — using local filesystem storage");
            }

            let acme_challenge_inner = AcmeChallengeService::new(redis.clone());
            let tls_resolver = TlsResolver::new(config_provider.clone(), acme_challenge_inner, redis.clone())
                .await
                .expect("failed to create tls resolver");

            (redis, tls_resolver)
        });

    let acme_challenge = AcmeChallengeService::new(redis.clone());
    let mut acme_challenge_service =
        Service::new("acme challenge service".to_string(), acme_challenge);

    acme_challenge_service.add_tcp("0.0.0.0:7765");

    server.add_service(acme_challenge_service);

    // Determine if TLS will be enabled to configure the HTTP→HTTPS redirect.
    let tls_enabled = std::env::var("ACME_EMAIL").is_ok();
    let proxy = SwarmProxy::new(gateway.clone(), tls_enabled);
    let mut proxy_service = http_proxy_service(&server.configuration, proxy);

    proxy_service.add_tcp("0.0.0.0:80");

    if let Some(tls_resolver) = tls_resolver {
        proxy_service.add_tls_with_settings("0.0.0.0:443", None, tls_resolver.as_tls_settings());
    }

    server.add_service(proxy_service);

    let config_refresher = ConfigRefresher::new(config_provider, gateway);
    let config_service = background_service("config refresher", config_refresher);

    server.add_service(config_service);

    server.run_forever()
}

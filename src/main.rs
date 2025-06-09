use config::ConfigRefresher;
use config::provider::docker::DockerConfig;
use pingora::prelude::*;
use pingora::services::listening::Service;
use tls::TlsResolver;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use self::proxy::Gateway;
use self::proxy::SwarmProxy;
use self::tls::AcmeChallengeService;

mod config;
mod proxy;
mod tls;

fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let mut server = Server::new(None).unwrap();

    let gateway = Gateway::default();
    let config_provider = DockerConfig::new().unwrap();
    let acme_challenge = AcmeChallengeService::default();

    let mut acme_challenge_service =
        Service::new("acme challenge service".to_string(), acme_challenge.clone());

    acme_challenge_service.add_tcp("0.0.0.0:7765");

    server.add_service(acme_challenge_service);

    let proxy = SwarmProxy::new(gateway.clone());
    let mut proxy_service = http_proxy_service(&server.configuration, proxy);

    proxy_service.add_tcp("0.0.0.0:80");

    if let Some(tls_resolver) = TlsResolver::new(config_provider.clone(), acme_challenge).unwrap() {
        proxy_service.add_tls_with_settings("0.0.0.0:443", None, tls_resolver.as_tls_settings());
    }

    server.add_service(proxy_service);

    let config_refresher = ConfigRefresher::new(config_provider, gateway);
    let config_service = background_service("config refresher", config_refresher);

    server.add_service(config_service);

    server.run_forever()
}

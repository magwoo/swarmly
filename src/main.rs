use config::ConfigRefresher;
use config::provider::docker::DockerConfig;
use pingora::prelude::*;

use self::proxy::Gateway;
use self::proxy::SwarmProxy;

mod config;
mod proxy;
mod tls;

fn main() {
    let mut server = Server::new(None).unwrap();

    let gateway = Gateway::default();
    let config_provider = DockerConfig::new().unwrap();

    let proxy = SwarmProxy::new(gateway.clone());
    let mut proxy_service = http_proxy_service(&server.configuration, proxy);

    proxy_service.add_tcp("0.0.0.0:80");

    server.add_service(proxy_service);

    let config_refresher = ConfigRefresher::new(config_provider, gateway);
    let config_service = background_service("config refresher", config_refresher);

    server.add_service(config_service);

    // gateway.add_tls_with_settings(
    //     "0.0.0.0:443",
    //     None,
    //     TlsSettings::with_callbacks(Box::new(tls::TlsResolver)).unwrap(),
    // );

    server.run_forever()
}

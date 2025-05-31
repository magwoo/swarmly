use pingora::listeners::tls::TlsSettings;
use pingora::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

mod config;
mod docker;
mod proxy;
mod tls;

fn main() {
    // let mut server = Server::new(None).unwrap();

    // let domains = Arc::new(RwLock::new(HashMap::default()));

    // let mut gateway = http_proxy_service(&server.configuration, Gateway(domains.clone()));
    // gateway.add_tcp("0.0.0.0:80");
    // gateway.add_tls_with_settings(
    //     "0.0.0.0:443",
    //     None,
    //     TlsSettings::with_callbacks(Box::new(tls::TlsResolver)).unwrap(),
    // );

    // let me_background = background_service("me", me::MeBackground(domains));

    // server.add_service(gateway);
    // server.add_service(me_background);

    // server.run_forever()
}

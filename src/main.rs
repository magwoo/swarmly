use pingora::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::gateway::Gateway;

mod docker;
mod gateway;
mod me;

fn main() {
    let mut server = Server::new(None).unwrap();

    let domains = Arc::new(RwLock::new(HashMap::default()));

    let mut gateway = http_proxy_service(&server.configuration, Gateway(domains.clone()));
    gateway.add_tcp("0.0.0.0:80");

    let me_background = background_service("me", me::MeBackground(domains));

    server.add_service(gateway);
    server.add_service(me_background);

    server.run_forever()
}

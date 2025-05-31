pub use self::gateway::Gateway;

mod gateway;

pub struct SwarmProxy(Gateway);

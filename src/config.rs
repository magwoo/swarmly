use self::provider::ConfigProvider;

pub mod provider;

pub struct ConfigRefresher<P>(P);

impl<P: ConfigProvider> ConfigRefresher<P> {
    pub fn new(provider: P) -> Self {
        Self(provider)
    }
}

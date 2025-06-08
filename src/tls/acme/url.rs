use acme_lib::DirectoryUrl;

pub trait UrlFromEnv
where
    Self: Sized,
{
    fn from_env() -> Option<Self>;
}

impl UrlFromEnv for DirectoryUrl<'static> {
    fn from_env() -> Option<Self> {
        let value: &'static str = std::env::var("ACME_PROVIDER").ok()?.leak().trim();

        let url = match value {
            "none" => return None,
            "letsencrypt" => DirectoryUrl::LetsEncrypt,
            "letsencrypt-staging" => DirectoryUrl::LetsEncryptStaging,
            url => DirectoryUrl::Other(url),
        };

        Some(url)
    }
}

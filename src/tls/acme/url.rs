use acme_lib::DirectoryUrl;

pub trait UrlFromEnv
where
    Self: Sized,
{
    fn from_env() -> Self;
}

impl UrlFromEnv for DirectoryUrl<'static> {
    fn from_env() -> Self {
        let value: &'static str = std::env::var("ACME_PROVIDER")
            .unwrap_or_else(|_| "none".to_owned())
            .leak()
            .trim();

        match value {
            "none" => DirectoryUrl::Other(""),
            "letsencrypt" => DirectoryUrl::LetsEncrypt,
            "letsencrypt-staging" => DirectoryUrl::LetsEncryptStaging,
            url => DirectoryUrl::Other(url),
        }
    }
}

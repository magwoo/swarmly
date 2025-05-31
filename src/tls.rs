use pingora::listeners::TlsAccept;
use pingora::protocols::tls::TlsRef;
use pingora::tls::pkey::PKey;
use pingora::tls::ssl::NameType;
use pingora::tls::x509::X509;

static DEV_CRT: &[u8] = include_bytes!("../docker/dev.crt");
static DEV_KEY: &[u8] = include_bytes!("../docker/dev.key");

pub struct TlsResolver;

#[async_trait::async_trait]
impl TlsAccept for TlsResolver {
    async fn certificate_callback(&self, ssl: &mut TlsRef) -> () {
        println!(
            "called tls resolver, servername: {:?}",
            ssl.servername(NameType::HOST_NAME)
        );

        if let Some(domain) = ssl.servername(NameType::HOST_NAME) {
            let crt = X509::from_pem(DEV_CRT).unwrap();
            ssl.set_certificate(&crt).unwrap();

            let key = PKey::private_key_from_pem(DEV_KEY).unwrap();
            ssl.set_private_key(&key).unwrap();
        }
    }
}

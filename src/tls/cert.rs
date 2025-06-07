use std::io::{BufWriter, Write};

use anyhow::Context;
use pingora::tls::pkey::{PKey, Private};
use pingora::tls::x509::X509;

pub struct Certificate {
    private_key: PKey<Private>,
    certificate: X509,
    order_timestamp: u64,
}

impl Certificate {
    pub fn new(pkey: &[u8], cert: &[u8], timestamp: u64) -> anyhow::Result<Self> {
        let private_key =
            PKey::private_key_from_pem(pkey).context("failed to parse private key as pem")?;

        let certificate = X509::from_pem(cert).context("failed to parse cert as pem")?;

        Ok(Self {
            private_key,
            certificate,
            order_timestamp: timestamp,
        })
    }

    pub fn private_key(&self) -> &PKey<Private> {
        &self.private_key
    }

    pub fn certificate(&self) -> &X509 {
        &self.certificate
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BufWriter::new(Vec::new());

        let pkey = self.private_key.private_key_to_pem_pkcs8().unwrap();
        let cert = self.certificate.to_pem().unwrap();

        buf.write_all(&self.order_timestamp.to_le_bytes()).unwrap();
        buf.write_all(&(pkey.len() as u64).to_le_bytes()).unwrap();
        buf.write_all(&(cert.len() as u64).to_le_bytes()).unwrap();
        buf.write_all(&pkey).unwrap();
        buf.write_all(&cert).unwrap();

        buf.into_inner().expect("we use simply vector")
    }

    pub fn from_bytes(buf: &[u8]) -> anyhow::Result<Self> {
        if buf.len() < 24 {
            anyhow::bail!("buffer too short for read head")
        }

        let head = &buf[..24];

        let timestamp = u64::from_le_bytes(head[..8].try_into().unwrap());
        let pkey_len = u64::from_le_bytes(head[8..16].try_into().unwrap());
        let cert_len = u64::from_le_bytes(head[16..].try_into().unwrap());

        let overall_len = head.len() + (pkey_len + cert_len) as usize;
        if buf.len() != overall_len {
            anyhow::bail!(
                "unexpected buffer len, expected: {}, current: {}",
                buf.len(),
                overall_len
            )
        }

        let body = &buf[head.len()..];

        let (pkey, cert) = body.split_at(pkey_len as usize);

        Self::new(pkey, cert, timestamp)
    }
}

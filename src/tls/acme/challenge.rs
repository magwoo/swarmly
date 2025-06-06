use acme_lib::create_p384_key;
use acme_lib::order::NewOrder;
use acme_lib::persist::MemoryPersist;
use anyhow::Context;
use std::sync::{Arc, Mutex};

use crate::tls::cert::Certificate;

type Order<P = MemoryPersist> = NewOrder<P>;

pub struct AcmeOrder {
    order: Arc<Mutex<Order>>,
}

pub struct AcmeChallenge {
    token: String,
    proof: String,
}

impl AcmeOrder {
    pub fn new(order: Order) -> Self {
        let order = Arc::new(Mutex::new(order));

        Self { order }
    }

    pub fn challenge_blocked(
        &self,
        challenge_callback: impl Fn(AcmeChallenge),
    ) -> anyhow::Result<Certificate> {
        let auths = self
            .order
            .lock()
            .unwrap()
            .authorizations()
            .context("failed to authorization")?;

        let auth = auths
            .into_iter()
            .next()
            .context("missing any acme authorizations")?;

        let challenge = auth.http_challenge();

        let token = challenge.http_token().to_owned();
        let proof = challenge.http_proof();

        challenge_callback(AcmeChallenge::new(token, proof));

        challenge
            .validate(2000)
            .context("failed to set validate delay")?;

        let mut order = self.order.lock().unwrap();

        let ord_csr = loop {
            if let Some(csr) = order.confirm_validations() {
                break csr;
            }

            order.refresh().context("failed to refresh order")?;
        };

        let pkey = create_p384_key();
        let ord_cert = ord_csr
            .finalize_pkey(pkey, 2000)
            .context("failed to finalaze cert")?;

        let cert = ord_cert
            .download_and_save_cert()
            .context("failed to download cert")?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("local time must be biggest when unix epoch")
            .as_secs();

        let cert = Certificate::new(
            cert.private_key().as_bytes(),
            cert.certificate().as_bytes(),
            timestamp,
        )
        .context("failed to create certificate")?;

        Ok(cert)
    }
}

impl AcmeChallenge {
    pub fn new(token: impl Into<String>, proof: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            proof: proof.into(),
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn proof(&self) -> &str {
        &self.proof
    }
}

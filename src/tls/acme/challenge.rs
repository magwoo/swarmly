use acme_lib::Certificate;
use acme_lib::persist::MemoryPersist;
use acme_lib::{create_p384_key, order::NewOrder};
use anyhow::Context;
use std::sync::{Arc, Mutex};

pub struct AcmeOrder {
    order: Arc<Mutex<NewOrder<MemoryPersist>>>,
}

pub struct AcmeChallenge {
    token: String,
    proof: String,
}

impl AcmeOrder {
    pub fn new(order: NewOrder<MemoryPersist>) -> Self {
        let order = Arc::new(Mutex::new(order));

        Self { order }
    }

    /// returns option (token, proof)
    pub fn get_challenge(&self) -> anyhow::Result<Option<AcmeChallenge>> {
        let auths = self
            .order
            .lock()
            .unwrap()
            .authorizations()
            .context("failed to authorization")?;

        let auth = match auths.into_iter().next() {
            Some(auth) => auth,
            None => return Ok(None),
        };

        let challenge = auth.http_challenge();

        let token = challenge.http_token().to_owned();
        let proof = challenge.http_proof();

        challenge
            .validate(2000)
            .context("failed to set validate delay")?;

        Ok(Some(AcmeChallenge::new(token, proof)))
    }

    pub fn validate_blocked(&self) -> anyhow::Result<Certificate> {
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
}

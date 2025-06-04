use acme_lib::Certificate;
use acme_lib::create_p384_key;
use acme_lib::order::NewOrder;
use acme_lib::persist::MemoryPersist;
use anyhow::Context;
use std::sync::{Arc, Mutex};

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
    ) -> anyhow::Result<Option<Certificate>> {
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

        Ok(Some(cert))
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

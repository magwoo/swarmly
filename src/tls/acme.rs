use anyhow::Context;
use instant_acme::{
    Account, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder, RetryPolicy,
};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::cert::Certificate;

pub mod service;

use service::AcmeChallengeService;

#[derive(Clone)]
pub struct AcmeResolver {
    account: OnceLock<Account>,
    contact: Option<String>,
    url: String,
}

impl AcmeResolver {
    const CHALLENGE_RETRY_POLICY: RetryPolicy = RetryPolicy::new()
        .initial_delay(Duration::from_millis(500))
        .timeout(Duration::from_secs(60));

    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let provider = match std::env::var("ACME_PROVIDER") {
            Ok(p) => p.trim().to_lowercase(),
            _ => return Ok(None),
        };

        let url = match provider.as_ref() {
            "letsencrypt" | "le" => LetsEncrypt::Production.url(),
            "staging-letsencrypt" | "sle" => LetsEncrypt::Staging.url(),
            other => other,
        }
        .to_owned();

        let contact = std::env::var("ACME_CONTACT").ok();

        Ok(Some(Self {
            account: OnceLock::new(),
            contact,
            url,
        }))
    }

    async fn account(&self) -> anyhow::Result<&Account> {
        if let Some(account) = self.account.get() {
            return Ok(account);
        }

        let contact_str = self.contact.as_ref().map(|m| format!("mailto:{m}"));
        let contact_arr;
        let contact: &[&str] = match &contact_str {
            Some(c) => {
                contact_arr = [c.as_str()];
                &contact_arr
            }
            None => &[],
        };

        let (account, _credentials) = Account::builder()
            .context("failed to create account builder")?
            .create(
                &NewAccount {
                    contact,
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                self.url.clone(),
                None,
            )
            .await
            .context("failed to create acme account")?;

        let _ = self.account.set(account);

        Ok(self.account.get().unwrap())
    }

    pub async fn issue_cert<D>(
        &self,
        domain: D,
        service: &AcmeChallengeService,
    ) -> anyhow::Result<Certificate>
    where
        D: Clone + Into<String> + std::fmt::Display,
    {
        tracing::debug!("ordering cert for domain: {}", domain);

        let identifiers = [Identifier::Dns(domain.clone().into())];
        let mut order = self
            .account()
            .await?
            .new_order(&NewOrder::new(&identifiers))
            .await
            .context("failed to create new order")?;

        {
            let mut authorizations = order.authorizations();
            let mut auth = authorizations
                .next()
                .await
                .context("no authorizations in order")?
                .context("failed to fetch authorization")?;

            let mut challenge = auth
                .challenge(ChallengeType::Http01)
                .context("missing http01 challenge")?;

            let token = challenge.token.clone();
            let proof = challenge.key_authorization().as_str().to_owned();

            service
                .store_challenge(&token, &proof)
                .await
                .context("failed to store acme challenge")?;

            challenge
                .set_ready()
                .await
                .context("failed to set challenge as ready")?;
        }

        let status = order
            .poll_ready(&Self::CHALLENGE_RETRY_POLICY)
            .await
            .context("challenge timed out or failed")?;

        tracing::debug!("order status for {}: {:?}", domain, status);

        let pkey_pem = order
            .finalize()
            .await
            .context("failed to finalize order")?;

        let cert_pem = order
            .poll_certificate(&RetryPolicy::new())
            .await
            .context("failed to retrieve certificate")?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("local time must be later than unix epoch")
            .as_secs();

        Certificate::new(pkey_pem.as_bytes(), cert_pem.as_bytes(), timestamp)
            .context("failed to create certificate")
    }
}

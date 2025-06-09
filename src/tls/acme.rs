use anyhow::Context;
use challenge::AcmeChallenge;
use instant_acme::{
    Account, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder, OrderStatus,
};
use rcgen::KeyPair;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::Sender;

use super::cert::Certificate;

pub mod challenge;
pub mod service;

#[derive(Clone)]
pub struct AcmeResolver {
    account: Account,
}

impl AcmeResolver {
    const PASS_CHALLENGE_ATTEMPTS: u32 = 10;
    const INITIAL_CHALLENGE_TIMEOUT: Duration = Duration::from_millis(250);

    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let provider = match std::env::var("ACME_PROVIDER") {
            Ok(provider) => provider.trim().to_lowercase(),
            _ => return Ok(None),
        };

        let url = match provider.as_ref() {
            "letsencrypt" | "le" => LetsEncrypt::Production.url(),
            "staging-letsencrypt" | "sle" => LetsEncrypt::Staging.url(),
            anyother => anyother,
        };

        let contact = std::env::var("ACME_CONTACT").map(|e| format!("mailto:{}", e));
        let contact = match contact.as_ref() {
            Ok(email) => &[email.as_str()] as &[&str],
            _ => &[],
        };

        // TODO: make an account save system and maybe real async
        let (account, _credentials) = futures::executor::block_on(async {
            Account::create(
                &NewAccount {
                    contact,
                    terms_of_service_agreed: true,
                    only_return_existing: true,
                },
                url,
                None,
            )
            .await
        })
        .context("failed to create account")?;

        Ok(Some(Self { account }))
    }

    pub async fn issue_cert<D>(
        &self,
        domain: D,
        channel: Sender<AcmeChallenge>,
    ) -> anyhow::Result<Certificate>
    where
        D: Clone + Into<String> + std::fmt::Display,
    {
        tracing::debug!("ordering domain: {}", domain);

        let mut order = self
            .account
            .new_order(&NewOrder {
                identifiers: &[Identifier::Dns(domain.clone().into())],
            })
            .await
            .context("failed to create new order")?;

        tracing::debug!("ordering domain: {}", domain);

        let challenges = order
            .authorizations()
            .await
            .context("failed to get authorizations")?
            .into_iter()
            .next()
            .context("missing any authorizations")?
            .challenges;

        let http_challenge = challenges
            .into_iter()
            .find(|c| c.r#type == ChallengeType::Http01)
            .context("missing http01 challenge")?;

        let proof = order.key_authorization(&http_challenge);
        let token = http_challenge.token;

        let challenge = AcmeChallenge::new(domain.clone(), token, proof.as_str());

        channel
            .send(challenge.clone())
            .await
            .context("failed to send challenge into channel")?;

        order
            .set_challenge_ready(&http_challenge.url)
            .await
            .context("failed to set challenge as ready for pass")?;

        for attempt in 1..Self::PASS_CHALLENGE_ATTEMPTS + 1 {
            let timeout = Self::INITIAL_CHALLENGE_TIMEOUT * (attempt);
            tracing::info!(
                "acme challenge for domain({}) attempt ({}/{}) waiting: {:?}",
                domain,
                attempt,
                Self::PASS_CHALLENGE_ATTEMPTS,
                timeout
            );

            tokio::time::sleep(timeout).await;

            let state = order
                .refresh()
                .await
                .context("failed to refresh order state")?;

            if state.status == OrderStatus::Ready {
                let private_key = KeyPair::generate().context("failed to generate csr keypair")?;
                let csr = rcgen::CertificateParams::new(vec![domain.clone().into()])
                    .context("failed to create csr")?
                    .serialize_request(&private_key)
                    .context("failed to serializer csr")?;

                order
                    .finalize(csr.der())
                    .await
                    .context("failed to finalize order")?;

                let cert = order
                    .certificate()
                    .await
                    .context("failed to get certificate")?
                    .context("missing certificate")?;

                let pkey = private_key.serialize_pem();
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("local time must be higher when unix epoch")
                    .as_secs();

                return Certificate::new(pkey.as_bytes(), cert.as_bytes(), timestamp)
                    .context("failed to create certificate");
            }
        }

        Err(anyhow::anyhow!(
            "challenge timed out with order status: {:?}",
            order.state().status
        ))
    }
}

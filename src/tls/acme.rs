use anyhow::Context;
use instant_acme::{Account, NewAccount};

use self::challenge::AcmeOrder;

pub mod challenge;
pub mod service;

#[derive(Clone)]
pub struct AcmeResolver {
    account: Account,
}

impl AcmeResolver {
    pub async fn from_env() -> anyhow::Result<Option<Self>> {
        let provider = match std::env::var("ACME_PROVIDER") {
            Ok(provider) => provider.trim().to_lowercase(),
            _ => return Ok(None),
        };

        let url = match provider.as_ref() {
            "letsencrypt" | "le" => instant_acme::LetsEncrypt::Production.url(),
            "staging-letsencrypt" | "sle" => instant_acme::LetsEncrypt::Staging.url(),
            anyother => anyother,
        };

        let contact = std::env::var("ACME_CONTACT").map(|e| format!("mailto:{}", e));
        let contact = match contact.as_ref() {
            Ok(email) => &[email.as_str()] as &[&str],
            _ => &[],
        };

        // TODO: make an account save system
        let (account, _credentials) = Account::create(
            &NewAccount {
                contact,
                terms_of_service_agreed: true,
                only_return_existing: true,
            },
            url,
            None,
        )
        .await
        .context("failed to create account")?;

        Ok(Some(Self { account }))
    }

    pub fn issue_cert(&self, domain: &str) -> anyhow::Result<AcmeOrder> {
        tracing::debug!("account dir preparing..");

        let acc = self
            .dir
            .account(&self.contact)
            .context("failed to account directory")?;

        tracing::debug!("ordering domain: {}", domain);

        let new_order = acc
            .new_order(domain, &[])
            .context("failed to order domain")?;

        Ok(AcmeOrder::new(new_order))
    }
}

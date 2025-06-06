use acme_lib::persist::MemoryPersist;
use acme_lib::{Directory, DirectoryUrl};
use anyhow::Context;

use self::challenge::AcmeOrder;

pub mod challenge;
pub mod service;

pub struct AcmeResolver {
    contact: String,
    dir: Directory<MemoryPersist>,
}

impl AcmeResolver {
    pub fn new(contact: impl Into<String>, url: DirectoryUrl<'static>) -> anyhow::Result<Self> {
        let persist = MemoryPersist::new();
        let dir = Directory::from_url(persist, url).context("failed to create directory")?;

        Ok(Self {
            contact: contact.into(),
            dir,
        })
    }

    pub fn issue_cert(&self, domain: &str) -> anyhow::Result<AcmeOrder> {
        let acc = self
            .dir
            .account(&self.contact)
            .context("failed to account directory")?;

        let new_order = acc
            .new_order(domain, &[])
            .context("failed to order domain")?;

        Ok(AcmeOrder::new(new_order))
    }
}

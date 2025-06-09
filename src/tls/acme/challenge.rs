#[derive(Clone)]
pub struct AcmeChallenge {
    domain: String,
    token: String,
    proof: String,
}

impl AcmeChallenge {
    pub fn new(
        domain: impl Into<String>,
        token: impl Into<String>,
        proof: impl Into<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            token: token.into(),
            proof: proof.into(),
        }
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn proof(&self) -> &str {
        &self.proof
    }
}

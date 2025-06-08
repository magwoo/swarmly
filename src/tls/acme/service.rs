use http::Response;
use pingora::apps::http_app::ServeHttp;
use pingora::protocols::http::ServerSession;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::challenge::AcmeChallenge;

#[derive(Default, Clone)]
pub struct AcmeChallengeService {
    challenges: Arc<RwLock<HashMap<String, AcmeChallenge>>>,
}

impl AcmeChallengeService {
    pub fn add_challenge(&self, domain: impl Into<String>, challenge: AcmeChallenge) {
        let mut challenges = self.challenges.write().unwrap();

        challenges.insert(domain.into(), challenge);
    }
}

#[async_trait::async_trait]
impl ServeHttp for AcmeChallengeService {
    async fn response(&self, session: &mut ServerSession) -> Response<Vec<u8>> {
        let not_found = || {
            Response::builder()
                .status(404)
                .body(Vec::<u8>::default())
                .expect("response must be valiable")
        };

        let uri = &session.req_header().uri;

        let domain = session
            .get_header("host")
            .and_then(|h| h.to_str().ok())
            .or_else(|| uri.host());

        let domain = match domain {
            Some(domain) => domain,
            None => return not_found(),
        };

        let challenges = self.challenges.read().unwrap();

        let challenge = match challenges.get(domain) {
            Some(challenge) => challenge,
            None => return not_found(),
        };

        let path = session.req_header().uri.path();

        if path.trim_matches('/') == format!(".well-known/acme-challenge/{}", challenge.token()) {
            return Response::new(challenge.proof().as_bytes().to_vec());
        }

        not_found()
    }
}

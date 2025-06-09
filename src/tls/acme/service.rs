use http::Response;
use pingora::apps::http_app::ServeHttp;
use pingora::protocols::http::ServerSession;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::{RwLock, mpsc};

use super::challenge::AcmeChallenge;

#[derive(Clone, Default)]
pub struct AcmeChallengeService {
    challenges: Arc<RwLock<HashMap<String, AcmeChallenge>>>,
    sender: OnceLock<Sender<AcmeChallenge>>,
}

impl AcmeChallengeService {
    const CHALLENGE_LIFETIME: Duration = Duration::from_secs(30);

    pub fn channel(&self) -> Sender<AcmeChallenge> {
        if let Some(sender) = self.sender.get() {
            return sender.clone();
        }

        let (sender, mut reciver) = mpsc::channel::<AcmeChallenge>(8);

        let challenges = Arc::clone(&self.challenges);
        tokio::spawn(async move {
            while let Some(challenge) = reciver.recv().await {
                let domain = challenge.domain().to_owned();

                challenges
                    .write()
                    .await
                    .insert(domain.to_owned(), challenge);

                let challenges_for_remove = Arc::clone(&challenges);
                tokio::spawn(async move {
                    tokio::time::sleep(Self::CHALLENGE_LIFETIME).await;

                    challenges_for_remove.write().await.remove(&domain);
                });
            }
        });

        self.sender
            .set(sender.clone())
            .expect("cell must be does not initialized");

        sender
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

        let challenges = self.challenges.read().await;

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

// impl Default for AcmeChallengeService {
//     fn default() -> Self {
//         let challenges = Arc::new(RwLock::new(HashMap::default()));

//         let (sender, mut reciver) = mpsc::channel::<AcmeChallenge>(8);

//         let challenges_for_add = Arc::clone(&challenges);
//         tokio::spawn(async move {
//             while let Some(challenge) = reciver.recv().await {
//                 let domain = challenge.domain().to_owned();

//                 let mut challenges = challenges_for_add.write().await;
//                 challenges.insert(domain.to_owned(), challenge);

//                 let challenges_for_remove = Arc::clone(&challenges_for_add);
//                 tokio::spawn(async move {
//                     tokio::time::sleep(Self::CHALLENGE_LIFETIME).await;

//                     challenges_for_remove.write().await.remove(&domain);
//                 });
//             }
//         });

//         Self { challenges, sender }
//     }
// }

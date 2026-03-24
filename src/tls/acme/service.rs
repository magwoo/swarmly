use http::Response;
use pingora::apps::http_app::ServeHttp;
use pingora::protocols::http::ServerSession;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::redis::RedisClient;

const CHALLENGE_KEY_PREFIX: &str = "swarmly:challenge:";
const CHALLENGE_TTL_SECS: u64 = 60;
const MEMORY_CHALLENGE_LIFETIME: Duration = Duration::from_secs(60);

enum ChallengeBackend {
    Memory(Arc<RwLock<HashMap<String, String>>>),
    Redis(RedisClient),
}

#[derive(Clone)]
pub struct AcmeChallengeService {
    backend: Arc<ChallengeBackend>,
}

impl AcmeChallengeService {
    pub fn new(redis: Option<RedisClient>) -> Self {
        let backend = match redis {
            Some(client) => ChallengeBackend::Redis(client),
            None => ChallengeBackend::Memory(Arc::new(RwLock::new(HashMap::new()))),
        };
        Self {
            backend: Arc::new(backend),
        }
    }

    pub async fn store_challenge(&self, token: &str, proof: &str) -> anyhow::Result<()> {
        match self.backend.as_ref() {
            ChallengeBackend::Redis(client) => {
                let key = challenge_key(token);
                client
                    .set(&key, proof.as_bytes().to_vec(), CHALLENGE_TTL_SECS)
                    .await
            }
            ChallengeBackend::Memory(map) => {
                let token = token.to_owned();
                let proof = proof.to_owned();
                let map_clone = Arc::clone(map);

                map.write().await.insert(token.clone(), proof);

                tokio::spawn(async move {
                    tokio::time::sleep(MEMORY_CHALLENGE_LIFETIME).await;
                    map_clone.write().await.remove(&token);
                });

                Ok(())
            }
        }
    }
}

#[async_trait::async_trait]
impl ServeHttp for AcmeChallengeService {
    async fn response(&self, session: &mut ServerSession) -> Response<Vec<u8>> {
        let not_found = || {
            Response::builder()
                .status(404)
                .body(Vec::<u8>::default())
                .expect("response must be valid")
        };

        let path = session.req_header().uri.path();

        let token = match path.strip_prefix("/.well-known/acme-challenge/") {
            Some(t) if !t.is_empty() => t,
            _ => return not_found(),
        };

        match self.backend.as_ref() {
            ChallengeBackend::Redis(client) => {
                let key = challenge_key(token);
                match client.get(&key).await {
                    Ok(Some(proof)) => Response::new(proof),
                    Ok(None) => not_found(),
                    Err(err) => {
                        tracing::error!("failed to get acme challenge from redis: {err:?}");
                        not_found()
                    }
                }
            }
            ChallengeBackend::Memory(map) => match map.read().await.get(token) {
                Some(proof) => Response::new(proof.as_bytes().to_vec()),
                None => not_found(),
            },
        }
    }
}

fn challenge_key(token: &str) -> String {
    format!("{}{}", CHALLENGE_KEY_PREFIX, token)
}

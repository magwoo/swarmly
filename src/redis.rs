use anyhow::Context;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;

#[derive(Clone)]
pub struct RedisClient {
    manager: ConnectionManager,
}

impl RedisClient {
    pub async fn from_env() -> anyhow::Result<Option<Self>> {
        let url = match std::env::var("REDIS_URL") {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };

        let client = redis::Client::open(url).context("failed to create redis client")?;
        let manager = ConnectionManager::new(client)
            .await
            .context("failed to connect to redis")?;

        Ok(Some(Self { manager }))
    }

    pub async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let mut conn = self.manager.clone();
        conn.get(key).await.context("redis GET failed")
    }

    pub async fn set(&self, key: &str, value: Vec<u8>, ttl_secs: u64) -> anyhow::Result<()> {
        let mut conn = self.manager.clone();
        conn.set_ex(key, value, ttl_secs).await.context("redis SET EX failed")
    }

    pub async fn set_nx(&self, key: &str, value: Vec<u8>, ttl_secs: u64) -> anyhow::Result<bool> {
        let mut conn = self.manager.clone();
        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await
            .context("redis SET NX EX failed")?;
        Ok(result.is_some())
    }

    pub async fn del(&self, key: &str) -> anyhow::Result<()> {
        let mut conn = self.manager.clone();
        conn.del(key).await.context("redis DEL failed")
    }
}

use redis::AsyncCommands;

/// drone-service 自有的 Redis 客户端；不复用主服务的 RedisBus/RedisStore。
#[derive(Clone)]
pub struct DroneRedis {
    conn: redis::aio::MultiplexedConnection,
}

impl DroneRedis {
    pub async fn connect(url: &str) -> anyhow::Result<DroneRedis> {
        let client = redis::Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(DroneRedis { conn })
    }

    /// 取一个可用的连接副本（多路复用连接可并发使用）。
    pub fn connection(&self) -> redis::aio::MultiplexedConnection {
        self.conn.clone()
    }

    pub async fn script_load(&self, script: &str) -> anyhow::Result<String> {
        let mut conn = self.conn.clone();
        let sha: String = redis::cmd("SCRIPT")
            .arg("LOAD")
            .arg(script)
            .query_async(&mut conn)
            .await?;
        Ok(sha)
    }

    pub async fn eval_sha(
        &self,
        sha: &str,
        keys: &[&str],
        args: &[&str],
    ) -> anyhow::Result<Vec<String>> {
        let mut conn = self.conn.clone();
        let mut cmd = redis::cmd("EVALSHA");
        cmd.arg(sha).arg(keys.len());
        for k in keys {
            cmd.arg(*k);
        }
        for a in args {
            cmd.arg(*a);
        }
        let out: Vec<String> = cmd.query_async(&mut conn).await?;
        Ok(out)
    }

    pub async fn set_nx_ex(&self, key: &str, value: &str, ttl_secs: u64) -> anyhow::Result<bool> {
        let mut conn = self.conn.clone();
        let ok: bool = redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await?;
        Ok(ok)
    }

    pub async fn set_hash(
        &self,
        key: &str,
        fields: &[(&str, &str)],
        ttl_secs: Option<u64>,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        for (f, v) in fields {
            let _: () = conn.hset(key, *f, *v).await?;
        }
        if let Some(ttl) = ttl_secs {
            let _: () = conn.expire(key, ttl as i64).await?;
        }
        Ok(())
    }

    pub async fn get_hash(
        &self,
        key: &str,
    ) -> anyhow::Result<std::collections::HashMap<String, String>> {
        let mut conn = self.conn.clone();
        let map: std::collections::HashMap<String, String> = conn.hgetall(key).await?;
        Ok(map)
    }

    pub async fn hget(&self, key: &str, field: &str) -> anyhow::Result<Option<String>> {
        let mut conn = self.conn.clone();
        let v: Option<String> = conn.hget(key, field).await?;
        Ok(v)
    }

    pub async fn expire(&self, key: &str, ttl_secs: u64) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        let _: () = conn.expire(key, ttl_secs as i64).await?;
        Ok(())
    }

    pub async fn xadd(
        &self,
        stream: &str,
        maxlen: Option<usize>,
        fields: &[(&str, &str)],
    ) -> anyhow::Result<String> {
        let mut conn = self.conn.clone();
        let mut cmd = redis::cmd("XADD");
        cmd.arg(stream);
        if let Some(n) = maxlen {
            cmd.arg("MAXLEN").arg(format!("~{n}"));
        }
        for (f, v) in fields {
            cmd.arg(*f).arg(*v);
        }
        let id: String = cmd.query_async(&mut conn).await?;
        Ok(id)
    }

    pub async fn xlen(&self, stream: &str) -> anyhow::Result<usize> {
        let mut conn = self.conn.clone();
        let n: usize = conn.xlen(stream).await?;
        Ok(n)
    }
}

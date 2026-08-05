// Redis Client Module
// 用于会话管理、短信频率限制等场景

use redis::{AsyncCommands, Client};

/// Redis 客户端封装
///
/// 提供异步 Redis 操作接口，支持：

#[derive(Clone)]
pub struct RedisClient {
    client: Client,
}

impl RedisClient {
    /// 创建新的 Redis 客户端
    pub fn new(url: &str) -> Result<Self, redis::RedisError> {
        Ok(Self { client: Client::open(url)? })
    }

    /// 获取键值
    pub async fn get(&self, key: &str) -> Result<Option<String>, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.get(key).await
    }

    /// 设置键值（带过期时间）
    pub async fn set_ex(&self, key: &str, value: &str, secs: u64) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.set_ex::<_, _, ()>(key, value, secs).await?;
        Ok(())
    }

    /// 自增计数器
    pub async fn incr(&self, key: &str) -> Result<i64, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.incr(key, 1).await
    }

    /// 获取键的 TTL
    pub async fn ttl(&self, key: &str) -> Result<i64, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.ttl(key).await
    }

    /// 删除键
    pub async fn del(&self, key: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.del::<_, ()>(key).await?;
        Ok(())
    }

    /// 检查键是否存在
    pub async fn exists(&self, key: &str) -> Result<bool, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.exists(key).await
    }

    /// 设置键的过期时间（秒）
    pub async fn expire(&self, key: &str, secs: i64) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.expire::<_, ()>(key, secs).await?;
        Ok(())
    }
}

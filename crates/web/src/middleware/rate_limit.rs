// 速率限制中间件
// 使用滑动窗口算法实现 API 速率限制

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header::HeaderName},
    middleware::Next,
    response::Response,
};
use tokio::sync::RwLock;

/// 速率限制配置
#[derive(Clone)]
pub struct RateLimitConfig {
    /// 每分钟最大请求数
    pub requests_per_minute: u32,
    /// 速率限制排除的路径
    pub exclude_paths: Vec<String>,
    /// 速率限制排除的方法
    pub exclude_methods: Vec<Method>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            exclude_paths: vec![
                "/health".to_string(),
                "/v1/auth/sms/send".to_string(), // 验证码发送单独限制
            ],
            exclude_methods: vec![Method::OPTIONS],
        }
    }
}

/// 速率限制器
#[derive(Clone)]
pub struct RateLimiter {
    /// 客户端请求记录: key = client_id, value = (请求时间列表, 第一次被限制的时间)
    #[allow(clippy::type_complexity)]
    records: Arc<RwLock<HashMap<String, (Vec<Instant>, Option<Instant>)>>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            records: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// 检查是否应该限制请求
    pub async fn check_rate_limit(&self, client_id: &str) -> RateLimitResult {
        let mut records = self.records.write().await;
        let now = Instant::now();
        let window = Duration::from_secs(60);

        let (requests, blocked_until) = records.entry(client_id.to_string()).or_insert((Vec::new(), None));

        // 如果之前被限制，检查是否还有效
        if let Some(until) = blocked_until {
            if now < *until {
                let remaining = until.duration_since(now).as_secs();
                return RateLimitResult::Blocked {
                    retry_after: remaining as u32,
                    message: format!("Too many requests. Please try again in {} seconds", remaining),
                };
            } else {
                // 限制期已过，清除限制
                *blocked_until = None;
                requests.clear();
            }
        }

        // 清理超过窗口期的请求
        requests.retain(|&time| now.duration_since(time) < window);

        // 检查是否超过限制
        if requests.len() >= self.config.requests_per_minute as usize {
            // 设置 60 秒的限制期
            *blocked_until = Some(now + Duration::from_secs(60));
            return RateLimitResult::Blocked {
                retry_after: 60,
                message: "Too many requests. Please try again in 60 seconds".to_string(),
            };
        }

        // 记录这次请求
        requests.push(now);

        RateLimitResult::Allowed {
            remaining: self.config.requests_per_minute as usize - requests.len(),
            reset_in: 60,
        }
    }

    /// 从请求中提取客户端标识
    pub fn get_client_id(request: &Request) -> String {
        // 优先使用 IP 地址
        if let Some(forwarded) = request.headers().get("x-forwarded-for")
            && let Ok(ip) = forwarded.to_str()
        {
            return ip.split(',').next().unwrap_or("unknown").to_string();
        }

        if let Some(real_ip) = request.headers().get("x-real-ip")
            && let Ok(ip) = real_ip.to_str()
        {
            return ip.to_string();
        }

        // 默认使用 "unknown"
        "unknown".to_string()
    }
}

/// 速率限制结果
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed { remaining: usize, reset_in: u32 },
    Blocked { retry_after: u32, message: String },
}

/// 速率限制中间件
pub async fn rate_limit_middleware(rate_limiter: RateLimiter, request: Request<Body>, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    // 检查是否应该跳过速率限制
    let config = rate_limiter.config.clone();
    if config.exclude_paths.iter().any(|p| path.starts_with(p)) || config.exclude_methods.contains(&method) {
        return next.run(request).await;
    }

    let client_id = RateLimiter::get_client_id(&request);

    match rate_limiter.check_rate_limit(&client_id).await {
        RateLimitResult::Allowed { remaining, reset_in } => {
            let mut response = next.run(request).await;

            // 添加速率限制头
            response.headers_mut().insert(
                HeaderName::from_static("x-rateLimit-limit"),
                HeaderValue::from(config.requests_per_minute),
            );
            response.headers_mut().insert(
                HeaderName::from_static("x-rateLimit-remaining"),
                HeaderValue::from(remaining),
            );
            response.headers_mut().insert(
                HeaderName::from_static("x-rateLimit-reset"),
                HeaderValue::from(reset_in),
            );

            response
        }
        RateLimitResult::Blocked { retry_after, message } => {
            tracing::warn!("Rate limit exceeded for client: {} on {}", client_id, path);

            Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Content-Type", "application/json")
                .header("Retry-After", retry_after)
                .header(HeaderName::from_static("x-rateLimit-limit"), config.requests_per_minute)
                .header(HeaderName::from_static("x-rateLimit-remaining"), 0)
                .header(HeaderName::from_static("x-rateLimit-reset"), retry_after)
                .body(Body::from(
                    serde_json::json!({
                        "code": -1,
                        "msg": message,
                        "result": serde_json::Value::Null
                    })
                    .to_string(),
                ))
                .unwrap()
        }
    }
}

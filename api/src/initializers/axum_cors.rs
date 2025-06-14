use loco_rs::prelude::*;
use axum::{
    http::{HeaderName, HeaderValue, Method},
    Router as AxumRouter,
};

use tower_http::cors::{CorsLayer};

pub struct AxumCorsInitializer;


#[async_trait]
impl Initializer for AxumCorsInitializer {
    fn name(&self) -> String {
        "axum-cors".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
        let cors = CorsLayer::new()
            .allow_origin("http://localhost:3000".parse::<HeaderValue>().unwrap())
            .allow_methods(vec![Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
            .allow_headers(vec![HeaderName::from_static("authorization"), HeaderName::from_static("content-type")])
            .allow_credentials(true);
        let router = router.layer(cors);
        Ok(router)
    }
}
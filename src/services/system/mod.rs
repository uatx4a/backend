use afast::handler;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Serialize, Deserialize, afast::AFastSerialize, afast::AFastDeserialize, afast::Tag,
)]
#[tag("健康检查响应")]
pub struct HealthResponse {
    pub status: String,
}

#[handler(desc("检查后端服务状态"))]
async fn health() -> afast::Result<HealthResponse> {
    Ok(HealthResponse {
        status: "ok".to_owned(),
    })
}

pub fn build_service() -> afast::Service {
    afast::service!("system", "系统服务" => {
        h(health),
    })
}

use afaster::orm::AFasterAdmin;
use afastorm::{FromRow, Model};

#[derive(Clone, Debug, FromRow, Model, AFasterAdmin)]
#[orm(table = "users")]
#[admin(label = "用户", list, detail)]
pub struct User {
    #[orm(primary_key)]
    #[admin(label = "用户 ID", list, order = 1, detail, readonly)]
    pub user_id: String,
    #[orm(unique)]
    #[admin(hidden)]
    pub openid: String,
    #[admin(label = "昵称", list, order = 2, detail, readonly)]
    pub nickname: String,
    #[orm(unique)]
    #[admin(hidden)]
    pub phone_number: Option<String>,
    #[admin(label = "实名状态", list, order = 4, detail, readonly)]
    pub real_name_status: String,
    #[admin(label = "创建时间（毫秒）", detail, readonly)]
    pub created_at_ms: i64,
    #[admin(label = "更新时间（毫秒）", detail, readonly)]
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, FromRow, Model)]
#[orm(table = "auth_sessions")]
pub struct AuthSession {
    #[orm(primary_key)]
    pub token_hash: String,
    pub user_id: String,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
}

#[derive(Clone, Debug, FromRow, Model)]
#[orm(table = "user_agreement_acceptances")]
pub struct UserAgreementAcceptance {
    #[orm(primary_key)]
    pub acceptance_id: String,
    pub user_id: String,
    pub agreement_version: String,
    pub accepted_at_ms: i64,
}

pub fn registry() -> afastorm::ModelRegistry {
    afastorm::ModelRegistry::new()
        .register::<User>()
        .register::<AuthSession>()
        .register::<UserAgreementAcceptance>()
}

pub fn admin_registry() -> afaster::orm::AdminModelRegistry {
    afaster::orm::AdminModelRegistry::new().register::<User>()
}

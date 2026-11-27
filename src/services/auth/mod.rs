pub mod models;

use std::time::{SystemTime, UNIX_EPOCH};

use afast::{AFastDeserialize, AFastSerialize, Custom, Data, State, Tag, handler};
use afastorm::Field;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

use self::models::{AuthSession, User, UserAgreementAcceptance};

const CODE_AUTH_REQUIRED: i64 = 41001;
const CODE_WX_LOGIN: i64 = 41003;
const CODE_AGREEMENT_REQUIRED: i64 = 41004;
const CODE_PHONE_AUTH: i64 = 41005;
const CODE_PHONE_ALREADY_BOUND: i64 = 41006;
const CODE_PHONE_CURRENT_ACCOUNT_BOUND: i64 = 41007;
const CODE_SERVER_ERROR: i64 = 51001;
const SESSION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, AFastDeserialize, Tag)]
#[tag("认证上下文")]
pub struct AuthCustom {
    pub token: String,
}

#[derive(Debug, AFastDeserialize, Tag)]
#[tag("微信小程序登录请求")]
pub struct LoginMiniRequest {
    pub code: String,
    pub nickname: String,
    pub agreement_version: String,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("登录用户资料")]
pub struct AuthUserProfile {
    pub user_id: String,
    pub nickname: String,
    pub avatar_url: String,
    pub phone_masked: String,
    pub phone_bound: bool,
    pub real_name_status: String,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("微信小程序登录响应")]
pub struct LoginMiniResponse {
    pub token: String,
    pub expires_at_ms: i64,
    pub is_new_user: bool,
    pub user: AuthUserProfile,
}

#[derive(Debug, AFastDeserialize, AFastSerialize, Tag)]
#[tag("空请求")]
pub struct EmptyRequest {}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("当前用户响应")]
pub struct GetMeResponse {
    pub user: AuthUserProfile,
}

#[derive(Debug, AFastDeserialize, Tag)]
#[tag("绑定手机号请求")]
pub struct BindPhoneRequest {
    pub code: String,
    pub confirm_transfer: bool,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("绑定手机号响应")]
pub struct BindPhoneResponse {
    pub phone_masked: String,
    pub phone_bound: bool,
    pub transferred: bool,
}

enum PhoneBindingOutcome {
    NeedsTransferConfirmation,
    CurrentAccountAlreadyBound,
    Bound { transferred: bool },
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("退出登录响应")]
pub struct LogoutResponse {
    pub revoked: bool,
}

#[handler(desc("通过微信 code 登录并创建平台会话"))]
async fn login_mini(
    State(state): State<afaster::AppState>,
    Data(request): Data<LoginMiniRequest>,
) -> afast::Result<LoginMiniResponse> {
    let code = request.code.trim();
    let agreement_version = request.agreement_version.trim();
    if code.is_empty() || agreement_version.is_empty() {
        return Err(afast::Error::custom(
            CODE_AGREEMENT_REQUIRED,
            "微信登录 code 和协议版本不能为空",
        ));
    }
    if request.nickname.chars().count() > 64 {
        return Err(afast::Error::custom(
            CODE_AGREEMENT_REQUIRED,
            "昵称长度不能超过 64 个字符",
        ));
    }

    let login = state.wxlogin.mini_login(code).await.map_err(|error| {
        tracing::warn!(error = %error, "AFaster wxlogin rejected the mini-program login");
        wx_login_error()
    })?;
    let openid = login.openid;
    let db = state.orm.inner();
    let existing_user = find_user_by_openid(db, &openid).await?;
    let mut is_new_user = existing_user.is_none();
    let nickname = if request.nickname.trim().is_empty() {
        "微信用户"
    } else {
        request.nickname.trim()
    };
    let mut user = if let Some(user) = existing_user {
        user
    } else {
        let now = unix_time_ms();
        let mut candidate = User {
            user_id: random_token("usr_")?,
            openid: openid.clone(),
            nickname: nickname.to_owned(),
            phone_number: None,
            real_name_status: "not_started".to_owned(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        match db
            .repo::<User>()
            .map_err(database_error)?
            .insert(&mut candidate)
            .await
        {
            Ok(_) => candidate,
            Err(error) => {
                if let Some(user) = find_user_by_openid(db, &openid).await? {
                    is_new_user = false;
                    user
                } else {
                    return Err(database_error(error));
                }
            }
        }
    };
    if user.nickname != nickname {
        user.nickname = nickname.to_owned();
        user.updated_at_ms = unix_time_ms();
        let updated_rows = db
            .repo::<User>()
            .map_err(database_error)?
            .update(&mut user, None)
            .await
            .map_err(database_error)?;
        if updated_rows == 0 {
            return Err(afast::Error::custom(CODE_SERVER_ERROR, "更新微信昵称失败"));
        }
    }

    let token = random_token("")?;
    let expires_at_ms = unix_time_ms() + SESSION_TTL_MS;
    let mut acceptance = UserAgreementAcceptance {
        acceptance_id: random_token("agr_")?,
        user_id: user.user_id.clone(),
        agreement_version: agreement_version.to_owned(),
        accepted_at_ms: unix_time_ms(),
    };
    let mut session = AuthSession {
        token_hash: hash_token(&token),
        user_id: user.user_id.clone(),
        expires_at_ms,
        created_at_ms: unix_time_ms(),
        revoked_at_ms: None,
    };
    db.transaction(async |transaction| {
        transaction
            .repo::<UserAgreementAcceptance>()?
            .insert(&mut acceptance)
            .await?;
        transaction
            .repo::<AuthSession>()?
            .insert(&mut session)
            .await?;
        Ok(())
    })
    .await
    .map_err(database_error)?;

    Ok(LoginMiniResponse {
        token,
        expires_at_ms,
        is_new_user,
        user: profile(user),
    })
}

#[handler(desc("读取当前登录用户资料"))]
async fn get_me(
    Custom(auth): Custom<AuthCustom>,
    State(state): State<afaster::AppState>,
    Data(_request): Data<EmptyRequest>,
) -> afast::Result<GetMeResponse> {
    let user = authenticate(state.orm.inner(), &auth.token).await?;
    Ok(GetMeResponse {
        user: profile(user),
    })
}

#[handler(desc("绑定手机号；经用户确认后可从其他账号转移"))]
async fn bind_phone(
    Custom(auth_custom): Custom<AuthCustom>,
    State(state): State<afaster::AppState>,
    Data(request): Data<BindPhoneRequest>,
) -> afast::Result<BindPhoneResponse> {
    let code = request.code.trim();
    if code.is_empty() {
        return Err(phone_auth_error());
    }
    let db = state.orm.inner();
    let user = authenticate(db, &auth_custom.token).await?;
    let phone_info = state
        .wxlogin
        .get_user_phone_number(code)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "AFaster wxlogin phone authorization failed");
            phone_auth_error()
        })?;
    let phone = phone_info.pure_phone_number;
    if !(5..=20).contains(&phone.len()) || !phone.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(phone_auth_error());
    }
    let phone_masked = mask_phone(&phone);

    let outcome = bind_phone_to_user(
        db,
        &user.user_id,
        &phone,
        request.confirm_transfer,
        unix_time_ms(),
    )
    .await?;

    match outcome {
        PhoneBindingOutcome::NeedsTransferConfirmation => Err(phone_already_bound_error()),
        PhoneBindingOutcome::CurrentAccountAlreadyBound => Err(phone_current_account_bound_error()),
        PhoneBindingOutcome::Bound { transferred } => Ok(BindPhoneResponse {
            phone_masked,
            phone_bound: true,
            transferred,
        }),
    }
}

async fn bind_phone_to_user<DB: afastorm::OrmDatabase>(
    db: &afastorm::Orm<DB>,
    user_id: &str,
    phone_number: &str,
    confirm_transfer: bool,
    updated_at_ms: i64,
) -> afast::Result<PhoneBindingOutcome>
where
    User: afastorm::Model<DB>,
    String: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    Option<String>: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    i64: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    for<'c> &'c mut DB::Connection: afastorm::sqlx::Executor<'c, Database = DB>,
{
    let user_id = user_id.to_owned();
    let phone_number = phone_number.to_owned();
    db.transaction(async |transaction| {
        let mut current_user = transaction
            .repo::<User>()?
            .find_by_id(user_id.clone())
            .await?
            .ok_or_else(|| {
                afastorm::Error::Query("current user disappeared during phone binding".into())
            })?;
        if let Some(current_phone) = current_user.phone_number.as_deref() {
            if current_phone == phone_number {
                return Ok(PhoneBindingOutcome::Bound { transferred: false });
            }
            return Ok(PhoneBindingOutcome::CurrentAccountAlreadyBound);
        }

        let existing_owner = transaction
            .repo::<User>()?
            .first_where(Field::new("phone_number").eq::<DB, _>(phone_number.clone()))
            .await?;
        let mut transferred = false;
        if let Some(mut existing_owner) = existing_owner {
            if existing_owner.user_id == user_id {
                return Ok(PhoneBindingOutcome::Bound { transferred: false });
            }
            if !confirm_transfer {
                return Ok(PhoneBindingOutcome::NeedsTransferConfirmation);
            }

            existing_owner.phone_number = None;
            existing_owner.updated_at_ms = updated_at_ms;
            let old_owner_updated = transaction
                .repo::<User>()?
                .update(&mut existing_owner, None)
                .await?;
            if old_owner_updated == 0 {
                return Err(afastorm::Error::Query(
                    "phone owner disappeared during transfer".into(),
                ));
            }
            transferred = true;
        }

        current_user.phone_number = Some(phone_number.clone());
        current_user.updated_at_ms = updated_at_ms;
        let current_user_updated = transaction
            .repo::<User>()?
            .update(&mut current_user, None)
            .await?;
        if current_user_updated == 0 {
            return Err(afastorm::Error::Query(
                "current user disappeared during phone binding".into(),
            ));
        }
        Ok(PhoneBindingOutcome::Bound { transferred })
    })
    .await
    .map_err(database_error)
}

#[handler(desc("撤销当前登录会话"))]
async fn logout(
    Custom(auth): Custom<AuthCustom>,
    State(state): State<afaster::AppState>,
    Data(_request): Data<EmptyRequest>,
) -> afast::Result<LogoutResponse> {
    let db = state.orm.inner();
    let sessions = db.repo::<AuthSession>().map_err(database_error)?;
    let Some(mut session) = sessions
        .find_by_id(hash_token(&auth.token))
        .await
        .map_err(database_error)?
    else {
        return Ok(LogoutResponse { revoked: false });
    };
    if session.revoked_at_ms.is_some() {
        return Ok(LogoutResponse { revoked: false });
    }
    session.revoked_at_ms = Some(unix_time_ms());
    let revoked = sessions
        .update(&mut session, None)
        .await
        .map_err(database_error)?
        > 0;
    Ok(LogoutResponse { revoked })
}

pub(crate) async fn authenticate<DB: afastorm::OrmDatabase>(
    db: &afastorm::Orm<DB>,
    token: &str,
) -> afast::Result<User>
where
    User: afastorm::Model<DB>,
    AuthSession: afastorm::Model<DB>,
    String: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    Option<String>: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    i64: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    Option<i64>: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    for<'c> &'c mut DB::Connection: afastorm::sqlx::Executor<'c, Database = DB>,
{
    if token.is_empty() {
        return Err(auth_required_error());
    }
    let sessions = db.repo::<AuthSession>().map_err(database_error)?;
    let session = sessions
        .find_by_id(hash_token(token))
        .await
        .map_err(database_error)?
        .filter(|session| session.revoked_at_ms.is_none() && session.expires_at_ms > unix_time_ms())
        .ok_or_else(auth_required_error)?;
    db.repo::<User>()
        .map_err(database_error)?
        .find_by_id(session.user_id)
        .await
        .map_err(database_error)?
        .ok_or_else(auth_required_error)
}

async fn find_user_by_openid<DB: afastorm::OrmDatabase>(
    db: &afastorm::Orm<DB>,
    openid: &str,
) -> afast::Result<Option<User>>
where
    User: afastorm::Model<DB>,
    String: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    Option<String>: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    i64: afastorm::sqlx::Type<DB>
        + for<'q> afastorm::sqlx::Encode<'q, DB>
        + for<'r> afastorm::sqlx::Decode<'r, DB>,
    for<'c> &'c mut DB::Connection: afastorm::sqlx::Executor<'c, Database = DB>,
{
    db.repo::<User>()
        .map_err(database_error)?
        .first_where(Field::new("openid").eq(openid.to_owned()))
        .await
        .map_err(database_error)
}

fn profile(user: User) -> AuthUserProfile {
    let phone_masked = user
        .phone_number
        .as_deref()
        .map(mask_phone)
        .unwrap_or_default();
    AuthUserProfile {
        user_id: user.user_id,
        nickname: user.nickname,
        avatar_url: String::new(),
        phone_bound: user.phone_number.is_some(),
        phone_masked,
        real_name_status: user.real_name_status,
    }
}

fn random_token(prefix: &str) -> afast::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| afast::Error::custom(CODE_SERVER_ERROR, "无法生成安全登录凭证"))?;
    Ok(format!("{prefix}{}", URL_SAFE_NO_PAD.encode(bytes)))
}

fn hash_token(token: &str) -> String {
    encode_hex(&Sha256::digest(token.as_bytes()))
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn mask_phone(phone: &str) -> String {
    if phone.len() <= 7 {
        return "*".repeat(phone.len());
    }
    format!("{}****{}", &phone[..3], &phone[phone.len() - 4..])
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn auth_required_error() -> afast::Error {
    afast::Error::custom(CODE_AUTH_REQUIRED, "登录状态无效或已过期")
}

fn wx_login_error() -> afast::Error {
    afast::Error::custom(CODE_WX_LOGIN, "微信登录失败，请重新尝试")
}

fn phone_auth_error() -> afast::Error {
    afast::Error::custom(CODE_PHONE_AUTH, "微信手机号验证失败，请重新授权")
}

fn phone_already_bound_error() -> afast::Error {
    afast::Error::custom(CODE_PHONE_ALREADY_BOUND, "该手机号已绑定其他账号")
}

fn phone_current_account_bound_error() -> afast::Error {
    afast::Error::custom(
        CODE_PHONE_CURRENT_ACCOUNT_BOUND,
        "当前账号已绑定其他手机号，不能直接替换",
    )
}

fn database_error(error: impl std::fmt::Display) -> afast::Error {
    tracing::error!(error = %error, "Authentication database operation failed");
    afast::Error::custom(CODE_SERVER_ERROR, "账号服务暂时不可用")
}

pub fn build_service() -> afast::Service {
    afast::service!("auth", "账号认证" => {
        h(login_mini),
        h(get_me),
        h(bind_phone),
        h(logout),
    })
}

#[cfg(all(test, feature = "test-sqlite"))]
mod tests {
    use super::*;

    async fn test_db() -> afastorm::Orm<afastorm::sqlx::Sqlite> {
        let db = afastorm::Orm::<afastorm::sqlx::Sqlite>::connect("sqlite::memory:")
            .await
            .unwrap()
            .with_models(models::registry());
        for statement in db.schema_statements().unwrap() {
            db.execute(&statement).await.unwrap();
        }
        db
    }

    fn test_user(user_id: &str, phone_number: Option<&str>) -> User {
        User {
            user_id: user_id.to_owned(),
            openid: format!("{user_id}-openid"),
            nickname: user_id.to_owned(),
            phone_number: phone_number.map(str::to_owned),
            real_name_status: "not_started".to_owned(),
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    async fn find_user(db: &afastorm::Orm<afastorm::sqlx::Sqlite>, user_id: &str) -> User {
        db.repo::<User>()
            .unwrap()
            .find_by_id(user_id.to_owned())
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn phone_transfer_requires_confirmation_then_unbinds_old_account_atomically() {
        let db = test_db().await;
        let mut old_user = test_user("old", Some("13800000000"));
        let mut current_user = test_user("current", None);
        db.repo::<User>()
            .unwrap()
            .insert(&mut old_user)
            .await
            .unwrap();
        db.repo::<User>()
            .unwrap()
            .insert(&mut current_user)
            .await
            .unwrap();

        let outcome = bind_phone_to_user(&db, "current", "13800000000", false, 2)
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            PhoneBindingOutcome::NeedsTransferConfirmation
        ));
        assert_eq!(
            find_user(&db, "old").await.phone_number.as_deref(),
            Some("13800000000")
        );
        assert_eq!(find_user(&db, "current").await.phone_number, None);

        let outcome = bind_phone_to_user(&db, "current", "13800000000", true, 3)
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            PhoneBindingOutcome::Bound { transferred: true }
        ));
        let old_user = find_user(&db, "old").await;
        assert_eq!(old_user.phone_number, None);
        let current_user = find_user(&db, "current").await;
        assert_eq!(current_user.phone_number.as_deref(), Some("13800000000"));
        let current_profile = profile(current_user);
        assert_eq!(current_profile.phone_masked, "138****0000");
    }

    #[tokio::test]
    async fn phone_transfer_does_not_replace_current_account_binding() {
        let db = test_db().await;
        let mut old_user = test_user("old", Some("13800000000"));
        let mut current_user = test_user("current", Some("13900000000"));
        db.repo::<User>()
            .unwrap()
            .insert(&mut old_user)
            .await
            .unwrap();
        db.repo::<User>()
            .unwrap()
            .insert(&mut current_user)
            .await
            .unwrap();

        let outcome = bind_phone_to_user(&db, "current", "13800000000", true, 2)
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            PhoneBindingOutcome::CurrentAccountAlreadyBound
        ));
        assert_eq!(
            find_user(&db, "old").await.phone_number.as_deref(),
            Some("13800000000")
        );
        assert_eq!(
            find_user(&db, "current").await.phone_number.as_deref(),
            Some("13900000000")
        );
    }
}

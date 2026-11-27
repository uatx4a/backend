pub mod models;

use std::time::{SystemTime, UNIX_EPOCH};

use afast::{AFastDeserialize, AFastSerialize, Custom, Data, State, Tag, handler};

use self::models::UploadedAsset;
use super::auth::{AuthCustom, authenticate};

const CODE_INVALID_UPLOAD: i64 = 43110;
const CODE_STORAGE_ERROR: i64 = 53120;

#[derive(Debug, AFastDeserialize, Tag)]
#[tag("获取文件上传授权请求")]
pub struct GetUploadFileLinkRequest {
    pub file_md5: String,
    pub extension: String,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("文件上传授权")]
pub struct GetUploadFileLinkResponse {
    pub file_md5: String,
    pub path: String,
    pub url: String,
    pub token: String,
    pub authorization: String,
    pub file_id: String,
    pub cos_file_id: String,
    pub openapi_seqid: Option<String>,
}

#[derive(Debug, AFastDeserialize, Tag)]
#[tag("确认文件上传请求")]
pub struct CompleteUploadRequest {
    pub file_md5: String,
    pub file_id: String,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("确认文件上传响应")]
pub struct CompleteUploadResponse {
    pub file_md5: String,
    pub file_id: String,
    pub uploaded: bool,
}

#[derive(Debug, AFastDeserialize, Tag)]
#[tag("获取文件临时访问地址请求")]
pub struct GetTemporaryAccessUrlRequest {
    pub file_md5: String,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("文件临时访问地址")]
pub struct GetTemporaryAccessUrlResponse {
    pub file_id: String,
    pub url: String,
    pub expires_in_seconds: u64,
    pub openapi_seqid: Option<String>,
}

#[handler(desc("为当前用户申请云托管对象存储上传授权"))]
async fn get_upload_file_link(
    Custom(auth): Custom<AuthCustom>,
    State(state): State<afaster::AppState>,
    Data(request): Data<GetUploadFileLinkRequest>,
) -> afast::Result<GetUploadFileLinkResponse> {
    let user = authenticate(state.orm.inner(), &auth.token).await?;
    let file_md5 = normalize_md5(&request.file_md5)?;
    let extension = normalize_extension(&request.extension)?;
    let relative_path = format!("users/{}/{}.{}", user.user_id, file_md5, extension);

    let link = state
        .wxcloudrun
        .get_upload_file_link(&relative_path)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "CloudRun rejected an upload-link request");
            storage_error("获取文件上传授权失败")
        })?;

    let asset_id = asset_id(&user.user_id, &file_md5);
    let now = unix_time_ms();
    let repository = state
        .orm
        .inner()
        .repo::<UploadedAsset>()
        .map_err(storage_database_error)?;
    if let Some(mut asset) = repository
        .find_by_id(asset_id.clone())
        .await
        .map_err(storage_database_error)?
    {
        asset.file_id = link.file_id.clone();
        asset.extension = extension;
        asset.upload_status = "pending".to_owned();
        asset.updated_at_ms = now;
        repository
            .update(&mut asset, None)
            .await
            .map_err(storage_database_error)?;
    } else {
        let mut asset = UploadedAsset {
            asset_id,
            user_id: user.user_id,
            file_md5: file_md5.clone(),
            file_id: link.file_id.clone(),
            extension,
            upload_status: "pending".to_owned(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        repository
            .insert(&mut asset)
            .await
            .map_err(storage_database_error)?;
    }

    Ok(GetUploadFileLinkResponse {
        file_md5,
        path: link.path,
        url: link.url,
        token: link.token,
        authorization: link.authorization,
        file_id: link.file_id,
        cos_file_id: link.cos_file_id,
        openapi_seqid: link.openapi_seqid,
    })
}

#[handler(desc("确认当前用户的对象存储文件上传完成"))]
async fn complete_upload(
    Custom(auth): Custom<AuthCustom>,
    State(state): State<afaster::AppState>,
    Data(request): Data<CompleteUploadRequest>,
) -> afast::Result<CompleteUploadResponse> {
    let user = authenticate(state.orm.inner(), &auth.token).await?;
    let file_md5 = normalize_md5(&request.file_md5)?;
    let asset_id = asset_id(&user.user_id, &file_md5);
    let repository = state
        .orm
        .inner()
        .repo::<UploadedAsset>()
        .map_err(storage_database_error)?;
    let mut asset = repository
        .find_by_id(asset_id)
        .await
        .map_err(storage_database_error)?
        .filter(|asset| asset.user_id == user.user_id && asset.file_id == request.file_id)
        .ok_or_else(|| invalid_upload("没有找到对应的上传授权"))?;

    state
        .wxcloudrun
        .get_temporary_access_url(&asset.file_id)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "CloudRun could not verify the uploaded file");
            storage_error("文件尚未上传成功")
        })?;

    asset.upload_status = "uploaded".to_owned();
    asset.updated_at_ms = unix_time_ms();
    repository
        .update(&mut asset, None)
        .await
        .map_err(storage_database_error)?;

    Ok(CompleteUploadResponse {
        file_md5,
        file_id: asset.file_id,
        uploaded: true,
    })
}

#[handler(desc("根据当前用户的文件 MD5 获取临时访问地址"))]
async fn get_temporary_access_url(
    Custom(auth): Custom<AuthCustom>,
    State(state): State<afaster::AppState>,
    Data(request): Data<GetTemporaryAccessUrlRequest>,
) -> afast::Result<GetTemporaryAccessUrlResponse> {
    let user = authenticate(state.orm.inner(), &auth.token).await?;
    let file_md5 = normalize_md5(&request.file_md5)?;
    let asset_id = asset_id(&user.user_id, &file_md5);
    let asset = state
        .orm
        .inner()
        .repo::<UploadedAsset>()
        .map_err(storage_database_error)?
        .find_by_id(asset_id)
        .await
        .map_err(storage_database_error)?
        .filter(|asset| asset.user_id == user.user_id && asset.upload_status == "uploaded")
        .ok_or_else(|| invalid_upload("没有找到已上传的文件"))?;

    let access = state
        .wxcloudrun
        .get_temporary_access_url(&asset.file_id)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "CloudRun could not issue a temporary file URL");
            storage_error("获取文件临时访问地址失败")
        })?;

    Ok(GetTemporaryAccessUrlResponse {
        file_id: access.file_id,
        url: access.url,
        expires_in_seconds: access.expires_in_seconds,
        openapi_seqid: access.openapi_seqid,
    })
}

fn normalize_md5(value: &str) -> afast::Result<String> {
    let value = value.trim();
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_upload("文件 MD5 必须是 32 位十六进制字符串"));
    }
    Ok(value.to_ascii_lowercase())
}

fn normalize_extension(value: &str) -> afast::Result<String> {
    let extension = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if !matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif") {
        return Err(invalid_upload("仅支持 jpg、jpeg、png、webp 或 gif 图片"));
    }
    Ok(extension)
}

fn asset_id(user_id: &str, file_md5: &str) -> String {
    format!("{user_id}_{file_md5}")
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn invalid_upload(message: &str) -> afast::Error {
    afast::Error::custom(CODE_INVALID_UPLOAD, message)
}

fn storage_error(message: &str) -> afast::Error {
    afast::Error::custom(CODE_STORAGE_ERROR, message)
}

fn storage_database_error(error: impl std::fmt::Display) -> afast::Error {
    tracing::error!(error = %error, "Storage database operation failed");
    storage_error("文件记录服务暂时不可用")
}

pub fn build_service() -> afast::Service {
    afast::service!("storage", "对象存储" => {
        h(get_upload_file_link),
        h(complete_upload),
        h(get_temporary_access_url),
    })
}

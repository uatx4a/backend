use afastorm::{FromRow, Model};

#[derive(Clone, Debug, FromRow, Model)]
#[orm(table = "uploaded_assets", index(user_id, file_md5), unique)]
pub struct UploadedAsset {
    #[orm(primary_key)]
    pub asset_id: String,
    pub user_id: String,
    pub file_md5: String,
    pub file_id: String,
    pub extension: String,
    pub upload_status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

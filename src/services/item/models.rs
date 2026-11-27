use afastorm::{FromRow, Model};

#[derive(Clone, Debug, FromRow, Model)]
#[orm(table = "items")]
pub struct Item {
    #[orm(primary_key)]
    pub item_id: String,
    pub owner_user_id: String,
    pub title: String,
    pub category_code: String,
    pub condition_code: String,
    pub description: String,
    pub valuation_method: String,
    pub value_low_fen: i64,
    pub value_high_fen: i64,
    pub valuation_source: String,
    pub valuation_expires_at_ms: i64,
    pub city_id: String,
    pub city_name: String,
    pub remote_only_face_to_face: bool,
    pub fulfillment_modes: String,
    pub serial_masked: String,
    pub serial_verified: bool,
    pub item_status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, FromRow, Model)]
#[orm(table = "item_images")]
pub struct ItemImage {
    #[orm(primary_key)]
    pub image_id: String,
    pub item_id: String,
    pub file_md5: String,
    pub file_id: String,
    pub sort_order: i32,
    pub created_at_ms: i64,
}

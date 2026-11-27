pub mod models;

use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use afast::{AFastDeserialize, AFastSerialize, Custom, Data, State, Tag, handler};
use afastorm::{Field, SortDirection};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use self::models::{Item, ItemImage};
use super::{
    auth::{AuthCustom, authenticate, models::User},
    storage::models::UploadedAsset,
};

const CODE_INVALID_ITEM: i64 = 43120;
const CODE_ITEM_ERROR: i64 = 53130;
const MAX_PAGE_SIZE: i32 = 50;
const MAX_IMAGES: usize = 9;
const VALUATION_VALIDITY_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const ITEM_IMAGE_WATERMARK_FONT_SIZE: u16 = 28;
const ITEM_IMAGE_WATERMARK_OPACITY: u8 = 50;

#[derive(Debug, AFastDeserialize, Tag)]
#[tag("物品列表请求")]
pub struct ListItemsRequest {
    pub city_id: String,
    pub nationwide: bool,
    pub category_code: String,
    pub keyword: String,
    pub page: i32,
    pub page_size: i32,
    pub sort_code: String,
}

#[derive(Debug, AFastDeserialize, Tag)]
#[tag("物品详情请求")]
pub struct GetItemRequest {
    pub item_id: String,
}

#[derive(Debug, AFastDeserialize, Tag)]
#[tag("发布物品请求")]
pub struct PublishItemRequest {
    pub title: String,
    pub category_code: String,
    pub condition_code: String,
    pub description: String,
    pub serial_number: String,
    pub valuation_method: String,
    pub value_low_fen: i64,
    pub value_high_fen: i64,
    pub city_id: String,
    pub city_name: String,
    pub fulfillment_modes: Vec<String>,
    pub photo_file_md5s: Vec<String>,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("物品发布者")]
pub struct ItemOwner {
    pub user_id: String,
    pub nickname: String,
    pub real_name_verified: bool,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("物品摘要")]
pub struct ItemSummary {
    pub item_id: String,
    pub title: String,
    pub category_code: String,
    pub condition_code: String,
    pub value_low_fen: i64,
    pub value_high_fen: i64,
    pub valuation_source: String,
    pub valuation_expires_at_ms: i64,
    pub city_id: String,
    pub city_name: String,
    pub remote_only_face_to_face: bool,
    pub fulfillment_modes: Vec<String>,
    pub cover_url: String,
    pub owner: ItemOwner,
    pub item_status: String,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("物品图片")]
pub struct ItemImageView {
    pub image_id: String,
    pub url: String,
    pub sort_order: i32,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("物品详情")]
pub struct ItemDetail {
    pub summary: ItemSummary,
    pub description: String,
    pub serial_masked: String,
    pub serial_verified: bool,
    pub images: Vec<ItemImageView>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub can_edit: bool,
    pub is_favorite: bool,
    pub price_alert_enabled: bool,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("物品列表")]
pub struct ListItemsResponse {
    pub items: Vec<ItemSummary>,
    pub page: i32,
    pub page_size: i32,
    pub total: i64,
}

#[derive(Debug, AFastSerialize, Tag)]
#[tag("发布物品响应")]
pub struct PublishItemResponse {
    pub item_id: String,
    pub item_status: String,
    pub created_at_ms: i64,
}

#[handler(desc("按地区、品类和关键词浏览已上架物品"))]
async fn list_items(
    State(state): State<afaster::AppState>,
    Data(request): Data<ListItemsRequest>,
) -> afast::Result<ListItemsResponse> {
    list_items_inner(&state, request, false).await
}

#[handler(desc("按标题关键词搜索已上架物品"))]
async fn search_items(
    State(state): State<afaster::AppState>,
    Data(request): Data<ListItemsRequest>,
) -> afast::Result<ListItemsResponse> {
    if request.keyword.trim().is_empty() {
        return Err(invalid_item("搜索关键词不能为空"));
    }
    list_items_inner(&state, request, true).await
}

#[handler(desc("获取已上架物品详情及图片临时访问地址"))]
async fn get_item(
    State(state): State<afaster::AppState>,
    Data(request): Data<GetItemRequest>,
) -> afast::Result<ItemDetail> {
    let item_id = request.item_id.trim();
    if item_id.is_empty() || item_id.len() > 64 {
        return Err(invalid_item("物品编号无效"));
    }
    let db = state.orm.inner();
    let item = db
        .repo::<Item>()
        .map_err(item_database_error)?
        .find_by_id(item_id.to_owned())
        .await
        .map_err(item_database_error)?
        .filter(|item| item.item_status == "active")
        .ok_or_else(|| invalid_item("物品不存在或已下架"))?;

    build_item_detail(&state, item, None).await
}

#[handler(desc("将当前用户已上传的图片发布为可浏览物品"))]
async fn publish_item(
    Custom(auth): Custom<AuthCustom>,
    State(state): State<afaster::AppState>,
    Data(request): Data<PublishItemRequest>,
) -> afast::Result<PublishItemResponse> {
    let user = authenticate(state.orm.inner(), &auth.token).await?;
    let validated = validate_publish_request(request)?;
    let db = state.orm.inner();
    let assets = load_uploaded_assets(db, &user.user_id, &validated.photo_file_md5s).await?;
    let now = unix_time_ms();
    let item = Item {
        item_id: random_id("itm_")?,
        owner_user_id: user.user_id.clone(),
        title: validated.title,
        category_code: validated.category_code,
        condition_code: validated.condition_code,
        description: validated.description,
        valuation_method: "manual".to_owned(),
        value_low_fen: validated.value_low_fen,
        value_high_fen: validated.value_high_fen,
        valuation_source: "用户手动估值".to_owned(),
        valuation_expires_at_ms: now.saturating_add(VALUATION_VALIDITY_MS),
        city_id: validated.city_id,
        city_name: validated.city_name,
        remote_only_face_to_face: validated.remote_only_face_to_face,
        fulfillment_modes: validated.fulfillment_modes.join(","),
        serial_masked: mask_serial(&validated.serial_number),
        serial_verified: false,
        item_status: "active".to_owned(),
        created_at_ms: now,
        updated_at_ms: now,
    };

    let mut transaction = db.begin().await.map_err(item_database_error)?;
    let insert_result = async {
        let mut item_to_insert = item.clone();
        transaction
            .repo::<Item>()
            .map_err(|error| error.to_string())?
            .insert(&mut item_to_insert)
            .await
            .map_err(|error| error.to_string())?;

        for (sort_order, asset) in assets.iter().enumerate() {
            let mut image = ItemImage {
                image_id: random_id("img_").map_err(|error| error.to_string())?,
                item_id: item.item_id.clone(),
                file_md5: asset.file_md5.clone(),
                file_id: asset.file_id.clone(),
                sort_order: sort_order as i32,
                created_at_ms: now,
            };
            transaction
                .repo::<ItemImage>()
                .map_err(|error| error.to_string())?
                .insert(&mut image)
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok::<(), String>(())
    }
    .await;

    if let Err(error) = insert_result {
        if let Err(rollback_error) = transaction.rollback().await {
            tracing::error!(
                error = %error,
                rollback_error = %rollback_error,
                "Item publish transaction rollback failed"
            );
        }
        tracing::error!(error = %error, "Item publish database transaction failed");
        return Err(item_error("发布物品失败，请稍后重试"));
    }
    transaction.commit().await.map_err(item_database_error)?;

    Ok(PublishItemResponse {
        item_id: item.item_id,
        item_status: item.item_status,
        created_at_ms: item.created_at_ms,
    })
}

async fn list_items_inner(
    state: &afaster::AppState,
    request: ListItemsRequest,
    search_only: bool,
) -> afast::Result<ListItemsResponse> {
    let db = state.orm.inner();
    let page = request.page.max(1);
    let page_size = request.page_size.clamp(1, MAX_PAGE_SIZE);
    let mut condition =
        Field::new("item_status").eq::<afastorm::DefaultDatabase, _>("active".to_owned());

    if !request.nationwide && !request.city_id.trim().is_empty() {
        condition = condition.and(Field::new("city_id").eq(request.city_id.trim().to_owned()));
    }
    if !request.category_code.trim().is_empty() && request.category_code != "all" {
        if !valid_category(request.category_code.trim()) {
            return Err(invalid_item("物品品类无效"));
        }
        condition =
            condition.and(Field::new("category_code").eq(request.category_code.trim().to_owned()));
    }
    let keyword = request.keyword.trim();
    if search_only && keyword.is_empty() {
        return Err(invalid_item("搜索关键词不能为空"));
    }
    if !keyword.is_empty() {
        condition = condition
            .and(Field::new("title").ilike::<afastorm::DefaultDatabase, _>(format!("%{keyword}%")));
    }
    if !matches!(request.sort_code.as_str(), "" | "newest") {
        return Err(invalid_item("排序方式无效"));
    }

    let repository = db.repo::<Item>().map_err(item_database_error)?;
    let total = repository
        .count_where(condition.clone())
        .await
        .map_err(item_database_error)?;
    let statement = repository
        .query()
        .filter(condition)
        .order_by_name("created_at_ms", SortDirection::Desc)
        .order_by_name("item_id", SortDirection::Desc)
        .limit(page_size as u64)
        .offset(((page - 1) as u64).saturating_mul(page_size as u64))
        .build()
        .map_err(item_database_error)?;
    let items = repository
        .fetch_all(&statement)
        .await
        .map_err(item_database_error)?;
    let summaries = build_item_summaries(state, &items).await?;

    Ok(ListItemsResponse {
        items: summaries,
        page,
        page_size,
        total,
    })
}

async fn build_item_summaries(
    state: &afaster::AppState,
    items: &[Item],
) -> afast::Result<Vec<ItemSummary>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let item_ids = items
        .iter()
        .map(|item| item.item_id.clone())
        .collect::<Vec<_>>();
    let db = state.orm.inner();
    let image_repo = db.repo::<ItemImage>().map_err(item_database_error)?;
    let image_statement = image_repo
        .query()
        .filter(Field::new("item_id").in_values(item_ids))
        .order_by_name("sort_order", SortDirection::Asc)
        .build()
        .map_err(item_database_error)?;
    let mut images = image_repo
        .fetch_all(&image_statement)
        .await
        .map_err(item_database_error)?;
    images.sort_by(|left, right| {
        left.item_id
            .cmp(&right.item_id)
            .then(left.sort_order.cmp(&right.sort_order))
    });
    let mut covers = HashMap::new();
    for image in images {
        covers.entry(image.item_id.clone()).or_insert(image);
    }

    let owner_ids = items
        .iter()
        .map(|item| item.owner_user_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let user_repo = db.repo::<User>().map_err(item_database_error)?;
    let user_statement = user_repo
        .query()
        .filter(Field::new("user_id").in_values(owner_ids))
        .build()
        .map_err(item_database_error)?;
    let users = user_repo
        .fetch_all(&user_statement)
        .await
        .map_err(item_database_error)?;
    let users = users
        .into_iter()
        .map(|user| (user.user_id.clone(), user))
        .collect::<HashMap<_, _>>();

    let cover_file_ids = items
        .iter()
        .filter_map(|item| covers.get(&item.item_id).map(|image| image.file_id.clone()))
        .collect::<Vec<_>>();
    let cover_urls = temporary_urls(state, &cover_file_ids).await?;

    items
        .iter()
        .map(|item| {
            let owner = users
                .get(&item.owner_user_id)
                .ok_or_else(|| item_error("物品发布者信息不存在"))?;
            Ok(item_summary(
                item,
                owner,
                covers
                    .get(&item.item_id)
                    .and_then(|image| cover_urls.get(&image.file_id)),
            ))
        })
        .collect()
}

async fn build_item_detail(
    state: &afaster::AppState,
    item: Item,
    viewer: Option<&User>,
) -> afast::Result<ItemDetail> {
    let db = state.orm.inner();
    let owner = if let Some(viewer) = viewer.filter(|viewer| viewer.user_id == item.owner_user_id) {
        viewer.clone()
    } else {
        db.repo::<User>()
            .map_err(item_database_error)?
            .find_by_id(item.owner_user_id.clone())
            .await
            .map_err(item_database_error)?
            .ok_or_else(|| item_error("物品发布者信息不存在"))?
    };
    let image_repo = db.repo::<ItemImage>().map_err(item_database_error)?;
    let image_statement = image_repo
        .query()
        .filter(Field::new("item_id").eq(item.item_id.clone()))
        .order_by_name("sort_order", SortDirection::Asc)
        .build()
        .map_err(item_database_error)?;
    let images = image_repo
        .fetch_all(&image_statement)
        .await
        .map_err(item_database_error)?;
    let file_ids = images
        .iter()
        .map(|image| image.file_id.clone())
        .collect::<Vec<_>>();
    let urls = temporary_urls(state, &file_ids).await?;
    let cover_image = images.first();
    let cover_url = cover_image.and_then(|image| urls.get(&image.file_id));
    let summary = item_summary(&item, &owner, cover_url);

    Ok(ItemDetail {
        summary,
        description: item.description,
        serial_masked: item.serial_masked,
        serial_verified: item.serial_verified,
        images: images
            .into_iter()
            .filter_map(|image| {
                urls.get(&image.file_id).map(|url| ItemImageView {
                    image_id: image.image_id,
                    url: url.clone(),
                    sort_order: image.sort_order,
                })
            })
            .collect(),
        created_at_ms: item.created_at_ms,
        updated_at_ms: item.updated_at_ms,
        can_edit: viewer.is_some_and(|viewer| viewer.user_id == item.owner_user_id),
        is_favorite: false,
        price_alert_enabled: false,
    })
}

async fn temporary_urls(
    state: &afaster::AppState,
    file_ids: &[String],
) -> afast::Result<HashMap<String, String>> {
    let unique_file_ids = file_ids.iter().cloned().collect::<HashSet<_>>();
    let mut pending_file_ids = unique_file_ids.into_iter();
    let mut tasks = tokio::task::JoinSet::new();
    let mut urls = HashMap::new();
    let image_process = item_image_watermark_process();

    for _ in 0..5 {
        if let Some(file_id) = pending_file_ids.next() {
            let cloudrun = state.wxcloudrun.clone();
            let image_process = image_process.clone();
            tasks.spawn(async move {
                cloudrun
                    .get_temporary_access_url_with_image_process(&file_id, &image_process)
                    .await
                    .map(|access| (file_id, access.url))
                    .map_err(|error| error.to_string())
            });
        }
    }

    while let Some(result) = tasks.join_next().await {
        let (file_id, url) = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "Could not issue temporary URL for item image");
                return Err(item_error("获取物品图片失败"));
            }
            Err(error) => {
                tracing::error!(error = %error, "Temporary URL task failed");
                return Err(item_error("获取物品图片失败"));
            }
        };
        urls.insert(file_id, url);

        if let Some(file_id) = pending_file_ids.next() {
            let cloudrun = state.wxcloudrun.clone();
            let image_process = image_process.clone();
            tasks.spawn(async move {
                cloudrun
                    .get_temporary_access_url_with_image_process(&file_id, &image_process)
                    .await
                    .map(|access| (file_id, access.url))
                    .map_err(|error| error.to_string())
            });
        }
    }

    Ok(urls)
}

fn item_image_watermark_process() -> String {
    format!(
        "watermark/2/text/{}/font/{}/fontsize/{}/fill/{}/dissolve/{}/gravity/southeast/dx/12/dy/12",
        URL_SAFE_NO_PAD.encode("换物集"),
        URL_SAFE_NO_PAD.encode("wqy-microhei"),
        ITEM_IMAGE_WATERMARK_FONT_SIZE,
        URL_SAFE_NO_PAD.encode("#FFFFFF"),
        ITEM_IMAGE_WATERMARK_OPACITY,
    )
}

fn item_summary(item: &Item, owner: &User, cover_url: Option<&String>) -> ItemSummary {
    ItemSummary {
        item_id: item.item_id.clone(),
        title: item.title.clone(),
        category_code: item.category_code.clone(),
        condition_code: item.condition_code.clone(),
        value_low_fen: item.value_low_fen,
        value_high_fen: item.value_high_fen,
        valuation_source: item.valuation_source.clone(),
        valuation_expires_at_ms: item.valuation_expires_at_ms,
        city_id: item.city_id.clone(),
        city_name: item.city_name.clone(),
        remote_only_face_to_face: item.remote_only_face_to_face,
        fulfillment_modes: item
            .fulfillment_modes
            .split(',')
            .map(str::to_owned)
            .collect(),
        cover_url: cover_url.cloned().unwrap_or_default(),
        owner: ItemOwner {
            user_id: owner.user_id.clone(),
            nickname: owner.nickname.clone(),
            real_name_verified: owner.real_name_status == "verified",
        },
        item_status: item.item_status.clone(),
    }
}

async fn load_uploaded_assets(
    db: &afastorm::Orm,
    user_id: &str,
    file_md5s: &[String],
) -> afast::Result<Vec<UploadedAsset>> {
    let repository = db.repo::<UploadedAsset>().map_err(item_database_error)?;
    let mut assets = Vec::with_capacity(file_md5s.len());
    for file_md5 in file_md5s {
        let asset_id = format!("{user_id}_{file_md5}");
        let asset = repository
            .find_by_id(asset_id)
            .await
            .map_err(item_database_error)?
            .filter(|asset| {
                asset.user_id == user_id
                    && asset.file_md5 == file_md5.as_str()
                    && asset.upload_status == "uploaded"
            })
            .ok_or_else(|| invalid_item("图片未上传完成，或不属于当前用户"))?;
        assets.push(asset);
    }
    Ok(assets)
}

#[derive(Debug)]
struct ValidatedPublish {
    title: String,
    category_code: String,
    condition_code: String,
    description: String,
    serial_number: String,
    value_low_fen: i64,
    value_high_fen: i64,
    city_id: String,
    city_name: String,
    remote_only_face_to_face: bool,
    fulfillment_modes: Vec<String>,
    photo_file_md5s: Vec<String>,
}

fn validate_publish_request(request: PublishItemRequest) -> afast::Result<ValidatedPublish> {
    let title = request.title.trim();
    if title.is_empty() || title.chars().count() > 30 {
        return Err(invalid_item("标题必须为 1 至 30 个字符"));
    }
    let category_code = request.category_code.trim();
    if !valid_category(category_code) {
        return Err(invalid_item("物品品类无效"));
    }
    let condition_code = request.condition_code.trim();
    if !valid_condition(condition_code) {
        return Err(invalid_item("物品成色无效"));
    }
    if request.description.chars().count() > 120 {
        return Err(invalid_item("瑕疵说明不能超过 120 个字符"));
    }
    if request.valuation_method != "manual" {
        return Err(invalid_item("目前仅支持手动填写估值区间"));
    }
    if request.value_low_fen <= 0 || request.value_high_fen <= request.value_low_fen {
        return Err(invalid_item("估值区间必须为正数，且下限小于上限"));
    }
    let value_midpoint =
        request.value_low_fen + (request.value_high_fen - request.value_low_fen) / 2;
    if value_midpoint > 300_000 {
        return Err(invalid_item(
            "高价值物品暂不可发布，待后端信用分能力接入后开放",
        ));
    }

    let city_id = request.city_id.trim();
    let city_name = request.city_name.trim();
    let (province, id_city) = city_id
        .split_once(':')
        .ok_or_else(|| invalid_item("交易地区无效"))?;
    if city_name.is_empty()
        || city_name == "全国"
        || city_name.chars().count() > 32
        || id_city != city_name
        || province.is_empty()
        || province.chars().count() > 16
    {
        return Err(invalid_item("交易地区信息不一致"));
    }
    if let Some(expected_province) = remote_province_for_city(city_name) {
        if expected_province != province {
            return Err(invalid_item("偏远地区与城市信息不一致"));
        }
    }
    let remote_only_face_to_face = is_remote_province(province);
    let mut fulfillment_modes = request
        .fulfillment_modes
        .iter()
        .map(|mode| mode.trim())
        .collect::<HashSet<_>>();
    if fulfillment_modes.is_empty()
        || fulfillment_modes
            .iter()
            .any(|mode| !matches!(*mode, "face" | "express"))
    {
        return Err(invalid_item("请至少选择一种有效的履约方式"));
    }
    if remote_only_face_to_face && fulfillment_modes.contains("express") {
        return Err(invalid_item("偏远地区物品仅支持当面交易"));
    }
    if remote_only_face_to_face {
        fulfillment_modes.insert("face");
    }
    let mut fulfillment_modes = fulfillment_modes
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    fulfillment_modes.sort();

    if request.photo_file_md5s.is_empty() || request.photo_file_md5s.len() > MAX_IMAGES {
        return Err(invalid_item("物品需要上传 1 至 9 张实拍图片"));
    }
    let mut seen_md5s = HashSet::new();
    let mut photo_file_md5s = Vec::with_capacity(request.photo_file_md5s.len());
    for value in request.photo_file_md5s {
        let md5 = normalize_md5(&value)?;
        if !seen_md5s.insert(md5.clone()) {
            return Err(invalid_item("图片不能重复"));
        }
        photo_file_md5s.push(md5);
    }

    let serial_number = request.serial_number.trim().to_ascii_uppercase();
    if !serial_number.is_empty() && !valid_serial(&serial_number) {
        return Err(invalid_item("序列号格式无效"));
    }

    Ok(ValidatedPublish {
        title: title.to_owned(),
        category_code: category_code.to_owned(),
        condition_code: condition_code.to_owned(),
        description: request.description.trim().to_owned(),
        serial_number,
        value_low_fen: request.value_low_fen,
        value_high_fen: request.value_high_fen,
        city_id: city_id.to_owned(),
        city_name: city_name.to_owned(),
        remote_only_face_to_face,
        fulfillment_modes,
        photo_file_md5s,
    })
}

fn valid_category(value: &str) -> bool {
    matches!(value, "digital" | "toy" | "book" | "home" | "clothing")
}

fn valid_condition(value: &str) -> bool {
    matches!(
        value,
        "new" | "like_new" | "excellent" | "good" | "fair" | "poor"
    )
}

fn valid_serial(value: &str) -> bool {
    (10..=12).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        || value.len() == 15 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn mask_serial(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        let last_four = value
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();
        format!("****{last_four}")
    }
}

fn normalize_md5(value: &str) -> afast::Result<String> {
    let value = value.trim();
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_item("图片 MD5 必须是 32 位十六进制字符串"));
    }
    Ok(value.to_ascii_lowercase())
}

fn remote_province_for_city(city: &str) -> Option<&'static str> {
    const REMOTE_CITIES: &[(&str, &str)] = &[
        ("呼和浩特", "内蒙古"),
        ("包头", "内蒙古"),
        ("乌海", "内蒙古"),
        ("赤峰", "内蒙古"),
        ("通辽", "内蒙古"),
        ("鄂尔多斯", "内蒙古"),
        ("呼伦贝尔", "内蒙古"),
        ("巴彦淖尔", "内蒙古"),
        ("乌兰察布", "内蒙古"),
        ("兴安", "内蒙古"),
        ("锡林郭勒", "内蒙古"),
        ("阿拉善", "内蒙古"),
        ("海口", "海南"),
        ("三亚", "海南"),
        ("三沙", "海南"),
        ("儋州", "海南"),
        ("拉萨", "西藏"),
        ("日喀则", "西藏"),
        ("昌都", "西藏"),
        ("林芝", "西藏"),
        ("山南", "西藏"),
        ("那曲", "西藏"),
        ("阿里", "西藏"),
        ("西宁", "青海"),
        ("海东", "青海"),
        ("海北", "青海"),
        ("黄南", "青海"),
        ("海南州", "青海"),
        ("果洛", "青海"),
        ("玉树", "青海"),
        ("海西", "青海"),
        ("银川", "宁夏"),
        ("石嘴山", "宁夏"),
        ("吴忠", "宁夏"),
        ("固原", "宁夏"),
        ("中卫", "宁夏"),
        ("乌鲁木齐", "新疆"),
        ("克拉玛依", "新疆"),
        ("吐鲁番", "新疆"),
        ("哈密", "新疆"),
        ("昌吉", "新疆"),
        ("博尔塔拉", "新疆"),
        ("巴音郭楞", "新疆"),
        ("阿克苏", "新疆"),
        ("克孜勒苏", "新疆"),
        ("喀什", "新疆"),
        ("和田", "新疆"),
        ("伊犁", "新疆"),
        ("塔城", "新疆"),
        ("阿勒泰", "新疆"),
    ];
    REMOTE_CITIES
        .iter()
        .find_map(|(name, province)| (*name == city).then_some(*province))
}

fn is_remote_province(province: &str) -> bool {
    matches!(
        province,
        "内蒙古" | "海南" | "西藏" | "青海" | "宁夏" | "新疆"
    )
}

fn random_id(prefix: &str) -> afast::Result<String> {
    let mut bytes = [0_u8; 18];
    getrandom::fill(&mut bytes).map_err(|error| {
        tracing::error!(error = %error, "Could not generate item identifier");
        item_error("生成物品编号失败")
    })?;
    Ok(format!("{prefix}{}", URL_SAFE_NO_PAD.encode(bytes)))
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn invalid_item(message: &str) -> afast::Error {
    afast::Error::custom(CODE_INVALID_ITEM, message)
}

fn item_error(message: &str) -> afast::Error {
    afast::Error::custom(CODE_ITEM_ERROR, message)
}

fn item_database_error(error: impl std::fmt::Display) -> afast::Error {
    tracing::error!(error = %error, "Item database operation failed");
    item_error("物品服务暂时不可用")
}

pub fn build_service() -> afast::Service {
    afast::service!("item", "物品" => {
        h(list_items),
        h(search_items),
        h(get_item),
        h(publish_item),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> PublishItemRequest {
        PublishItemRequest {
            title: "相机".to_owned(),
            category_code: "digital".to_owned(),
            condition_code: "excellent".to_owned(),
            description: "轻微使用痕迹".to_owned(),
            serial_number: "ABCD123456".to_owned(),
            valuation_method: "manual".to_owned(),
            value_low_fen: 100_00,
            value_high_fen: 200_00,
            city_id: "浙江:杭州".to_owned(),
            city_name: "杭州".to_owned(),
            fulfillment_modes: vec!["face".to_owned(), "express".to_owned()],
            photo_file_md5s: vec!["a".repeat(32)],
        }
    }

    #[test]
    fn accepts_publish_request_and_masks_serial() {
        let validated = validate_publish_request(request()).unwrap();

        assert_eq!(validated.city_id, "浙江:杭州");
        assert!(!validated.remote_only_face_to_face);
        assert_eq!(mask_serial(&validated.serial_number), "****3456");
    }

    #[test]
    fn blocks_express_delivery_for_remote_city() {
        let mut request = request();
        request.city_id = "新疆:乌鲁木齐".to_owned();
        request.city_name = "乌鲁木齐".to_owned();

        assert!(validate_publish_request(request).is_err());
    }

    #[test]
    fn rejects_unuploaded_shape_inputs_before_database_access() {
        let mut request = request();
        request.photo_file_md5s.clear();

        assert!(validate_publish_request(request).is_err());
    }

    #[test]
    fn blocks_high_value_without_server_side_credit_data() {
        let mut request = request();
        request.value_low_fen = 300_001;
        request.value_high_fen = 400_000;

        assert!(validate_publish_request(request).is_err());
    }

    #[test]
    fn item_image_watermark_is_translucent_and_bottom_right() {
        let process = item_image_watermark_process();
        let segments = process.split('/').collect::<Vec<_>>();

        assert_eq!(segments[0..3], ["watermark", "2", "text"]);
        assert_eq!(
            URL_SAFE_NO_PAD.decode(segments[3]).unwrap(),
            "换物集".as_bytes()
        );
        assert_eq!(segments[4], "font");
        assert_eq!(
            URL_SAFE_NO_PAD.decode(segments[5]).unwrap(),
            b"wqy-microhei"
        );
        assert_eq!(
            segments[6..],
            [
                "fontsize",
                "28",
                "fill",
                "I0ZGRkZGRg",
                "dissolve",
                "50",
                "gravity",
                "southeast",
                "dx",
                "12",
                "dy",
                "12"
            ]
        );
    }
}

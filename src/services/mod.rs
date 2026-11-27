pub mod auth;
pub mod item;
pub mod storage;
pub mod system;

pub fn model_registry() -> afastorm::ModelRegistry {
    auth::models::registry()
        .register::<storage::models::UploadedAsset>()
        .register::<item::models::Item>()
        .register::<item::models::ItemImage>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn uploaded_asset_unique_index_is_in_the_orm_schema() {
        let orm = afastorm::Orm::<sqlx::MySql>::connect_lazy("mysql://test:test@localhost/test")
            .expect("create lazy MySQL ORM")
            .with_models(model_registry());
        let snapshot = orm.schema_snapshot().expect("create schema snapshot");
        let index = snapshot
            .indexes
            .iter()
            .find(|index| index.name == "uq_uploaded_assets_user_id_file_md5")
            .expect("uploaded asset unique index is registered");

        assert!(index.create_sql.contains("CREATE UNIQUE INDEX"));
        assert!(index.create_sql.contains("`user_id`, `file_md5`"));
    }
}

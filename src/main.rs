use std::path::Path;

use afaster::AFaster;

use afaster::AFasterOrmExt;
use backend::services;

#[tokio::main]
async fn main() {
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("config.toml");
    let mut app = AFaster::new(config_path.to_string_lossy().into_owned())
        .await
        .expect("initialize AFaster")
        .with_orm(|orm| {
            orm.with_models(services::model_registry())
                .with_admin(true)
                .with_admin_brand_text("换换管理后台")
                .with_admin_models(services::auth::models::admin_registry())
        })
        .expect("register authentication ORM models");

    for (_, service) in services() {
        app = app.service(service);
    }

    if app
        .run_command_if_requested()
        .await
        .expect("run AFaster command")
    {
        return;
    }

    app.run().await;
}

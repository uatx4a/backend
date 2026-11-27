pub mod services;

use std::path::{Path, PathBuf};

use afast::{GenerateTarget, JsTsCallType, Lang};

pub fn services() -> Vec<(&'static str, afast::Service)> {
    vec![
        ("system", services::system::build_service()),
        ("auth", services::auth::build_service()),
        ("item", services::item::build_service()),
        ("storage", services::storage::build_service()),
    ]
}

pub fn client_target(service_name: &str) -> GenerateTarget {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend must be inside the workspace");

    GenerateTarget {
        lang: Lang::JS(vec![JsTsCallType::WxRequest]),
        path: PathBuf::from(workspace_root)
            .join("miniprogram")
            .join("api")
            .join(service_name),
        debug: true,
        services: Some(vec![service_name.to_owned()]),
    }
}

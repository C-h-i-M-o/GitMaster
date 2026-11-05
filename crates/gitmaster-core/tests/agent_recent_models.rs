use gitmaster_core::agent::*;

/// 最近模型去重并保留五项，跨重启恢复且忽略删除或禁用项。
#[test]
fn recent_models_are_bounded_persistent_and_only_include_enabled_models() {
    let directory = tempfile::tempdir().unwrap();
    let store = AgentStore::open(directory.path().into()).unwrap();
    for index in 0..6 {
        let id = format!("a{index}");
        store
            .save_profile(ProviderProfile {
                id: id.clone(),
                name: id.clone(),
                protocol: ProviderProtocol::Responses,
                base_url: "https://example.com/v1".into(),
                model: id.clone(),
                credential_id: "cc".into(),
            })
            .unwrap();
        store.remember_model(&id).unwrap();
    }
    assert_eq!(
        store.recent_model_ids().unwrap(),
        ["a5", "a4", "a3", "a2", "a1"]
    );
    store.remember_model("a3").unwrap();
    assert_eq!(
        store.recent_model_ids().unwrap(),
        ["a3", "a5", "a4", "a2", "a1"]
    );
    drop(store);
    let store = AgentStore::open(directory.path().into()).unwrap();
    assert_eq!(store.recent_model_ids().unwrap().len(), 5);
    store.delete_profile("a3").unwrap();
    assert!(!store.recent_model_ids().unwrap().contains(&"a3".to_owned()));
    let path = directory.path().join("settings.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for model in document["models"].as_array_mut().unwrap() {
        if model["id"] == "a4" {
            model["enabled"] = false.into();
        }
    }
    std::fs::write(path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(!store.recent_model_ids().unwrap().contains(&"a4".to_owned()));
    assert!(store.remember_model("a4").is_err());
}

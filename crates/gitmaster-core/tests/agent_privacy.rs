use gitmaster_core::agent::privacy::*;
use serde_json::json;
/// 常见凭据与当前 Key 在输入、文件正文和协议嵌套对象中均被阻止。
#[test]
fn credentials_do_not_enter_history() {
    for text in [
        "API_KEY=supersecretvalue",
        "OPENAI_API_KEY=supersecretvalue",
        "GITHUB_TOKEN=supersecretvalue",
        "https://name:password@example.com/a",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "https://example.com/?token=secret",
        "sk-12345678901234567890",
    ] {
        assert!(ensure_safe_text(text, None).is_err(), "{text}");
    }
    assert!(ensure_safe_value(&json!({"api_key":"supersecret"}), None).is_err());
    assert!(ensure_safe_value(
        &json!([{"text":"abc-current-secret-xyz"}]),
        Some("current-secret")
    )
    .is_err());
    assert!(ensure_safe_text("增加 API Key 配置；const credential_id = profile.id;", None).is_ok());
    assert!(ensure_safe_value(
        &json!({"encrypted_content":"opaque","signature":"opaque"}),
        None
    )
    .is_ok());
}

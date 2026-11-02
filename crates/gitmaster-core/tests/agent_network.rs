use gitmaster_core::agent::policy::PolicyDecision;
use gitmaster_core::agent::{provider::Cancellation, tools::network::NetworkTools, *};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
};

/// 私网读取必须经过策略批准，重定向作为新目标返回，不能偷偷跟随。
#[tokio::test]
async fn private_network_requires_approval_and_redirect_is_not_followed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..count]).to_lowercase();
        assert!(!request.contains("authorization:"));
        assert!(!request.contains("cookie:"));
        stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /other\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let call = ToolCall {
        id: "call_1".into(),
        name: "fetch_url".into(),
        arguments: json!({"url":url}),
    };
    let prepared = NetworkTools::prepare(&call, PermissionMode::AutoApprove)
        .await
        .unwrap();
    assert_eq!(prepared.decision, PolicyDecision::Ask);
    assert!(NetworkTools::execute(prepared, false, &Cancellation::new())
        .await
        .is_err());
    let prepared = NetworkTools::prepare(&call, PermissionMode::AutoApprove)
        .await
        .unwrap();
    let result = NetworkTools::execute(prepared, true, &Cancellation::new())
        .await
        .unwrap();
    assert_eq!(result["redirectUrl"], format!("{url}other"));
    worker.join().unwrap();
}

/// 用户信息、隐藏鉴权参数及非 HTTP 协议在联网前拒绝。
#[tokio::test]
async fn credential_urls_are_rejected_before_dns() {
    for url in [
        "file:///etc/passwd",
        "https://user:pass@example.com/",
        "https://example.com/?api_key=secret",
    ] {
        let call = ToolCall {
            id: "call_1".into(),
            name: "fetch_url".into(),
            arguments: json!({"url":url}),
        };
        assert!(NetworkTools::prepare(&call, PermissionMode::FullAccess)
            .await
            .is_err());
    }
}

/// 私网与特殊 IPv6 地址不能因不属于常见 IPv4 私网而自动获批。
#[tokio::test]
async fn special_ipv6_destinations_require_approval() {
    for address in [
        "::1",
        "::ffff:127.0.0.1",
        "::127.0.0.1",
        "fec0::1",
        "fc00::1",
        "64:ff9b::7f00:1",
        "2001:db8::1",
        "2002:7f00:1::1",
    ] {
        let call = ToolCall {
            id: "call_1".into(),
            name: "fetch_url".into(),
            arguments: json!({"url":format!("http://[{address}]:18765/")}),
        };
        let prepared = NetworkTools::prepare(&call, PermissionMode::AutoApprove)
            .await
            .unwrap();
        assert_eq!(prepared.decision, PolicyDecision::Ask, "{address}");
    }
}

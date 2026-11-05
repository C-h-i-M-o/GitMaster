use gitmaster_core::agent::{
    provider::Cancellation, tools::files::FileTools, PermissionMode, ToolCall,
};
use serde_json::json;

/// 工作区边界由工具强制，所有权限模式都不能写程序目录或访问未授权目录。
#[test]
fn default_workspace_has_readonly_application_and_separate_output() {
    let temporary = tempfile::tempdir().unwrap();
    let application = temporary.path().join("application");
    let output = temporary.path().join("output");
    let outside = temporary.path().join("outside.txt");
    std::fs::create_dir(&application).unwrap();
    std::fs::create_dir(&output).unwrap();
    std::fs::write(application.join("readme.txt"), "程序资料").unwrap();
    std::fs::write(&outside, "范围外资料").unwrap();
    let files = FileTools::scoped(application.clone(), output.clone(), vec![], None).unwrap();
    let read = files
        .prepare(
            &call("read_file", json!({"path":"readme.txt"})),
            PermissionMode::FullAccess,
        )
        .unwrap();
    assert_eq!(
        files.execute(read, true, &Cancellation::new()).unwrap()["content"],
        "程序资料"
    );
    for mode in [
        PermissionMode::RequestApproval,
        PermissionMode::AutoApprove,
        PermissionMode::FullAccess,
    ] {
        assert!(files
            .prepare(
                &call(
                    "write_file",
                    json!({"path":"program.txt","content":"不可写","expectedVersion":null})
                ),
                mode
            )
            .is_err());
        assert!(files
            .prepare(&call("read_file", json!({"path":outside})), mode)
            .is_err());
    }
    let write = files
        .prepare(
            &call(
                "write_file",
                json!({"path":output.join("result.txt"),"content":"生成物","expectedVersion":null}),
            ),
            PermissionMode::FullAccess,
        )
        .unwrap();
    files.execute(write, true, &Cancellation::new()).unwrap();
    assert_eq!(
        std::fs::read_to_string(output.join("result.txt")).unwrap(),
        "生成物"
    );
    assert!(!application.join("program.txt").exists());
}

/// 普通文件夹无需 Git 即可读写，但相对路径穿越不能得到范围外能力。
#[test]
fn plain_folder_scope_rejects_parent_traversal() {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path().join("project");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(temporary.path().join("secret.txt"), "外部资料").unwrap();
    let files = FileTools::scoped(folder.clone(), folder, vec![], None).unwrap();
    assert!(files
        .prepare(
            &call("read_file", json!({"path":"../secret.txt"})),
            PermissionMode::FullAccess
        )
        .is_err());
    let write = files
        .prepare(
            &call(
                "write_file",
                json!({"path":"new.txt","content":"本地文件夹","expectedVersion":null}),
            ),
            PermissionMode::FullAccess,
        )
        .unwrap();
    files.execute(write, true, &Cancellation::new()).unwrap();
}

/// 应用数据父目录保持保护，只对明确签发的生成物子目录开放。
#[test]
fn default_output_is_the_only_exception_inside_protected_application_data() {
    let temporary = tempfile::tempdir().unwrap();
    let app = temporary.path().join("app");
    let data = temporary.path().join("data");
    let output = data.join("agent-output");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(data.join("settings.json"), "私有配置").unwrap();
    let files = FileTools::scoped(app, output.clone(), vec![data.clone()], None).unwrap();
    assert!(files
        .prepare(
            &call("read_file", json!({"path":data.join("settings.json")})),
            PermissionMode::FullAccess
        )
        .is_err());
    let write = files
        .prepare(
            &call(
                "write_file",
                json!({"path":output.join("result.txt"),"content":"结果","expectedVersion":null}),
            ),
            PermissionMode::FullAccess,
        )
        .unwrap();
    files.execute(write, true, &Cancellation::new()).unwrap();
}

/// 只构造生产入口接受的工具参数。
fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: "workspace-call".into(),
        name: name.into(),
        arguments,
    }
}

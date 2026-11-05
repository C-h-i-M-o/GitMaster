use gitmaster_core::agent::{
    provider::Cancellation, tools::files::FileTools, PermissionMode, ToolCall,
};
use serde_json::{json, Value};

/// 只以本次写入前的实际内容作基线，不能把用户原有修改归给 Agent。
#[test]
fn file_change_records_actual_before_and_verified_after_without_git() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file.txt"), "用户已有改动\n").unwrap();
    let files = FileTools::scoped(root.path().into(), root.path().into(), vec![], None).unwrap();
    let read = execute(&files, "read_file", json!({"path":"file.txt"})).unwrap();
    let saved = execute(&files, "write_file", json!({"path":"file.txt","content":"用户已有改动\n本次增加\n","expectedVersion":read["version"]})).unwrap();
    assert_eq!(saved["change"]["beforeContent"], "用户已有改动\n");
    assert_eq!(
        saved["change"]["afterContent"],
        std::fs::read_to_string(root.path().join("file.txt")).unwrap()
    );
    assert_eq!(saved["change"]["beforeVersion"], read["version"]);
    assert_eq!(saved["change"]["afterVersion"], saved["version"]);
    assert_eq!(saved["change"]["verified"], true);
}

/// 外部编辑使已读版本失效，工具失败后不产生成功改动摘要。
#[test]
fn concurrent_edit_is_preserved_and_not_claimed_as_an_agent_change() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("file.txt");
    std::fs::write(&file, "旧内容").unwrap();
    let files = FileTools::scoped(root.path().into(), root.path().into(), vec![], None).unwrap();
    let read = execute(&files, "read_file", json!({"path":"file.txt"})).unwrap();
    std::fs::write(&file, "并发修改").unwrap();
    assert!(execute(
        &files,
        "write_file",
        json!({"path":"file.txt","content":"计划修改","expectedVersion":read["version"]})
    )
    .is_err());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "并发修改");
}

/// 新文件的基线明确不存在，二进制文件不冒充可展示的文本差异。
#[test]
fn new_file_has_explicit_missing_baseline_and_binary_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let files = FileTools::scoped(root.path().into(), root.path().into(), vec![], None).unwrap();
    let saved = execute(
        &files,
        "write_file",
        json!({"path":"new.txt","content":"新文件","expectedVersion":null}),
    )
    .unwrap();
    assert!(saved["change"]["beforeContent"].is_null());
    assert_eq!(saved["change"]["afterContent"], "新文件");
    std::fs::write(root.path().join("binary.bin"), [0u8, 255, 0]).unwrap();
    assert!(execute(&files, "read_file", json!({"path":"binary.bin"})).is_err());
}

/// 通过真实工具准备与执行生成证据，不直接替换生产结果。
fn execute(
    files: &FileTools,
    name: &str,
    arguments: Value,
) -> gitmaster_core::agent::AgentResult<Value> {
    let request = ToolCall {
        id: "aa".into(),
        name: name.into(),
        arguments,
    };
    let prepared = files.prepare(&request, PermissionMode::FullAccess)?;
    files.execute(prepared, true, &Cancellation::new())
}

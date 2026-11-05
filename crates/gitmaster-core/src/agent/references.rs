use crate::agent::{
    privacy::ensure_safe_text,
    provider::Cancellation,
    tools::registry::{ToolOutput, Toolbox},
    AgentError, AgentResult, PermissionMode, ToolCall,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

const MAX_REFERENCES: usize = 8;
const MAX_REFERENCE_BYTES: usize = 64 * 1024;

/// 用户在界面明确选择的一项文件、文本或 Git 差异引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserReferenceSpec {
    pub kind: ReferenceKind,
    pub path: Option<String>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub expected_version: Option<String>,
    pub text: Option<String>,
    pub staged: Option<bool>,
}

/// 引用来源种类；Git 差异名称按 IPC 合同序列化为 `gitDiff`。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceKind {
    File,
    Text,
    GitDiff,
}

/// 经后端权限、路径、版本与隐私校验后可进入本轮上下文的引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedReference {
    pub kind: ReferenceKind,
    pub label: String,
    pub path: Option<String>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub version: String,
    pub text: String,
}

/// 重验最多八项用户引用；文件与差异只能经现有只读工具路径读取。
pub async fn verify_references(
    toolbox: &Arc<Toolbox>,
    specs: &[UserReferenceSpec],
    cancel: &Cancellation,
) -> AgentResult<Vec<VerifiedReference>> {
    if specs.len() > MAX_REFERENCES {
        return Err(reference_error(
            "AGENT_REFERENCE_LIMIT",
            "每轮最多引用 8 项资料。",
        ));
    }
    let mut verified = Vec::with_capacity(specs.len());
    let mut total_bytes = 0usize;
    for (index, spec) in specs.iter().enumerate() {
        cancel.check()?;
        let reference = match spec.kind {
            ReferenceKind::Text => verify_text(spec)?,
            ReferenceKind::File => verify_file(toolbox, spec, index, cancel).await?,
            ReferenceKind::GitDiff => verify_git_diff(toolbox, spec, index, cancel).await?,
        };
        total_bytes = total_bytes.saturating_add(reference.text.len());
        if total_bytes > MAX_REFERENCE_BYTES {
            return Err(reference_error(
                "AGENT_REFERENCE_LIMIT",
                "引用资料合计不得超过 64 KiB。",
            ));
        }
        verified.push(reference);
    }
    cancel.check()?;
    Ok(verified)
}

/// 只接受用户显式提交的有界文本，并标明其来源是用户选区。
fn verify_text(spec: &UserReferenceSpec) -> AgentResult<VerifiedReference> {
    if spec.path.is_some()
        || spec.start_line.is_some()
        || spec.end_line.is_some()
        || spec.staged.is_some()
    {
        return Err(invalid_reference());
    }
    let text = spec.text.as_deref().ok_or_else(invalid_reference)?;
    if text.is_empty() {
        return Err(invalid_reference());
    }
    ensure_safe_text(text, None)?;
    let version = text_hash(text);
    check_expected_version(spec.expected_version.as_deref(), &version)?;
    Ok(VerifiedReference {
        kind: ReferenceKind::Text,
        label: "用户选中的文本".into(),
        path: None,
        start_line: None,
        end_line: None,
        version,
        text: text.to_owned(),
    })
}

/// 经工作区 FileTools 读取文件后校验整文件版本，再按 1 起始闭区间选行。
async fn verify_file(
    toolbox: &Arc<Toolbox>,
    spec: &UserReferenceSpec,
    index: usize,
    cancel: &Cancellation,
) -> AgentResult<VerifiedReference> {
    if spec.text.is_some() || spec.staged.is_some() {
        return Err(invalid_reference());
    }
    let path = spec.path.as_deref().ok_or_else(invalid_reference)?;
    validate_line_range(spec.start_line, spec.end_line)?;
    let call = ToolCall {
        id: format!("{:x}", index + 1),
        name: "read_file".into(),
        arguments: json!({"path":path}),
    };
    let prepared = toolbox
        .prepare(call, PermissionMode::RequestApproval)
        .await?;
    if prepared.decision != crate::agent::policy::PolicyDecision::Allow {
        return Err(reference_error(
            "AGENT_REFERENCE_PERMISSION",
            "所选文件当前不在可直接只读的工作区范围内。",
        ));
    }
    let output = toolbox.execute(prepared, false, cancel).await?;
    let value = result_value(output)?;
    let path = value["path"]
        .as_str()
        .ok_or_else(invalid_reference)?
        .to_owned();
    let content = value["content"].as_str().ok_or_else(invalid_reference)?;
    let version = value["version"]
        .as_str()
        .ok_or_else(invalid_reference)?
        .to_owned();
    check_expected_version(spec.expected_version.as_deref(), &version)?;
    let (text, start_line, end_line) = select_lines(content, spec.start_line, spec.end_line)?;
    ensure_safe_text(&text, None)?;
    Ok(VerifiedReference {
        kind: ReferenceKind::File,
        label: format!("用户选中文件：{path}:{start_line}-{end_line}"),
        path: Some(path),
        start_line: Some(start_line),
        end_line: Some(end_line),
        version,
        text,
    })
}

/// 经当前 GitTools 只读 diff 获取差异，并用内容哈希标识本次快照。
async fn verify_git_diff(
    toolbox: &Arc<Toolbox>,
    spec: &UserReferenceSpec,
    index: usize,
    cancel: &Cancellation,
) -> AgentResult<VerifiedReference> {
    if spec.text.is_some() || spec.start_line.is_some() || spec.end_line.is_some() {
        return Err(invalid_reference());
    }
    let staged = spec.staged.unwrap_or(false);
    let call = ToolCall {
        id: format!("{:x}", index + 1),
        name: "git_diff".into(),
        arguments: json!({"staged":staged,"path":spec.path.as_deref()}),
    };
    let prepared = toolbox
        .prepare(call, PermissionMode::RequestApproval)
        .await?;
    if prepared.decision != crate::agent::policy::PolicyDecision::Allow {
        return Err(reference_error(
            "AGENT_REFERENCE_PERMISSION",
            "当前 Git 差异不能以只读方式访问。",
        ));
    }
    let output = toolbox.execute(prepared, false, cancel).await?;
    let value = result_value(output)?;
    if value["truncated"].as_bool().unwrap_or(false) {
        return Err(reference_error(
            "AGENT_REFERENCE_LIMIT",
            "Git 差异超过单次读取上限，请选择具体文件后再引用。",
        ));
    }
    let text = value["text"].as_str().ok_or_else(invalid_reference)?;
    ensure_safe_text(text, None)?;
    let version = text_hash(text);
    check_expected_version(spec.expected_version.as_deref(), &version)?;
    let selection = spec
        .path
        .as_deref()
        .map(|value| format!("（{value}）"))
        .unwrap_or_default();
    Ok(VerifiedReference {
        kind: ReferenceKind::GitDiff,
        label: format!(
            "用户选中的{}Git 差异{selection}",
            if staged { "暂存区" } else { "工作区" }
        ),
        // 保留差异查询路径，方便发送前针对同一目标复核版本。
        path: spec.path.clone(),
        start_line: None,
        end_line: None,
        version,
        text: text.to_owned(),
    })
}

/// 确保范围为有效的一起始闭区间，缺省时由读取内容补成全文件范围。
fn validate_line_range(start: Option<usize>, end: Option<usize>) -> AgentResult<()> {
    if start == Some(0)
        || end == Some(0)
        || matches!((start, end), (Some(first), Some(last)) if first > last)
    {
        return Err(reference_error(
            "AGENT_REFERENCE_RANGE",
            "文件引用行号必须从 1 开始，且结束行不能早于起始行。",
        ));
    }
    Ok(())
}

/// 保留原始换行字节并按用户指定行号截取，不跨越文件范围。
fn select_lines(
    content: &str,
    requested_start: Option<usize>,
    requested_end: Option<usize>,
) -> AgentResult<(String, usize, usize)> {
    let lines = if content.is_empty() {
        vec![""]
    } else {
        content.split_inclusive('\n').collect::<Vec<_>>()
    };
    let start = requested_start.unwrap_or(1);
    let end = requested_end.unwrap_or(lines.len());
    if start == 0 || end == 0 || start > end || end > lines.len() {
        return Err(reference_error(
            "AGENT_REFERENCE_RANGE",
            "文件引用行号超出当前文件范围。",
        ));
    }
    Ok((lines[start - 1..end].concat(), start, end))
}

/// 将已执行工具结果统一为 JSON 对象；其他工具输出类型一律拒绝。
fn result_value(output: ToolOutput) -> AgentResult<serde_json::Value> {
    match output {
        ToolOutput::Result(value) => Ok(value),
        ToolOutput::Summary { .. } => Err(invalid_reference()),
    }
}

/// 用户给出的旧版本必须与本次真实读取一致，过期时不标记为当前内容。
fn check_expected_version(expected: Option<&str>, actual: &str) -> AgentResult<()> {
    if expected.is_some_and(|value| value != actual) {
        return Err(reference_error(
            "AGENT_REFERENCE_CHANGED",
            "引用内容已变化，请刷新预览后再发送。",
        ));
    }
    Ok(())
}

/// 用 SHA-256 绑定引用文字版本，便于发送前再次核验。
fn text_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// 非法规格只返回稳定错误码，不回显路径或用户文本。
fn invalid_reference() -> AgentError {
    reference_error("AGENT_REFERENCE_INVALID", "引用格式不正确或内容不可读取。")
}

/// 构造不包含用户引用原文的稳定错误。
fn reference_error(code: &str, message: &str) -> AgentError {
    AgentError::new(code, message)
}

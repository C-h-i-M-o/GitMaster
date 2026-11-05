use serde::{Deserialize, Serialize};

/// 单次结构化文件写入的已核验快照，按需读取而不进入普通模型上下文。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileChangeContent {
    pub path: String,
    pub before_content: Option<String>,
    pub after_content: String,
    pub before_version: Option<String>,
    pub after_version: String,
    pub verified: bool,
}

/// 持久事件只保存不可变工件引用，不把整个文件重复放进事件日志。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileChangeArtifact {
    pub artifact_id: String,
    pub digest: String,
    pub path: String,
    pub before_version: Option<String>,
    pub after_version: String,
    pub created: bool,
}

//! 终端、编辑器和外部打开的强类型设置；读取和校验不会运行程序。
use serde::{Deserialize, Serialize};

/// 终端启动目录由用户明确选择，与文件编辑的项目根限制独立。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum TerminalDirectory {
    Project,
    Home,
    Fixed { path: String },
}

/// 空执行路径代表应用自动发现系统 shell，不代表空命令。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalProfile {
    pub profile_id: String,
    pub name: String,
    pub executable_path: Option<String>,
    pub args: Vec<String>,
    pub cwd: TerminalDirectory,
}

/// 光标外观枚举，避免任意字符串进入终端组件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
}

/// 终端显示与启动配置，启动参数仅供新会话读取。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalPreferences {
    pub default_profile_id: String,
    pub profiles: Vec<TerminalProfile>,
    pub font_family: String,
    pub font_size: u8,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub scrollback_lines: u32,
}
impl Default for TerminalPreferences {
    /// 默认配置延迟到用户打开终端时才发现并运行系统 shell。
    fn default() -> Self {
        Self {
            default_profile_id: "system".into(),
            profiles: vec![TerminalProfile {
                profile_id: "system".into(),
                name: "系统默认".into(),
                executable_path: None,
                args: Vec::new(),
                cwd: TerminalDirectory::Project,
            }],
            font_family: "Consolas, Menlo, monospace".into(),
            font_size: 14,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            scrollback_lines: 5000,
        }
    }
}

/// 编辑器换行只影响显示，不改写文件中的换行字符。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WordWrap {
    Off,
    On,
}

/// 编辑器基础偏好，不包含文件编码转换与自动保存。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorPreferences {
    pub font_family: String,
    pub font_size: u8,
    pub tab_size: u8,
    pub word_wrap: WordWrap,
}
impl Default for EditorPreferences {
    /// 用系统等宽字体显示原有内容，默认不折行。
    fn default() -> Self {
        Self {
            font_family: "Consolas, Menlo, monospace".into(),
            font_size: 14,
            tab_size: 4,
            word_wrap: WordWrap::Off,
        }
    }
}

/// 外部启动只接受产品已支持的应用身份。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalAppId {
    FileManager,
    VsCode,
    Terminal,
}

/// 默认外部打开方式只影响 gitMaster，不改变系统文件关联。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalOpenPreferences {
    pub default_app_id: ExternalAppId,
}
impl Default for ExternalOpenPreferences {
    /// 文件管理器无需预先安装额外应用。
    fn default() -> Self {
        Self {
            default_app_id: ExternalAppId::FileManager,
        }
    }
}

/// 表单错误字段使用固定路径，消息不回显用户参数或敏感内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

/// 校验设置值的范围和引用关系；不扫描目录或执行 shell。
pub fn validate(settings: &super::Settings) -> Vec<FieldError> {
    let mut errors = Vec::new();
    let mut reject = |field: &str, message: &str| {
        errors.push(FieldError {
            field: field.into(),
            message: message.into(),
        })
    };
    if !(1..=10).contains(&settings.ui_preferences.elasticity) {
        reject("uiPreferences.elasticity", "节点弹性必须介于 1 到 10。");
    }
    for (field, font, size) in [
        (
            "terminal",
            &settings.terminal.font_family,
            settings.terminal.font_size,
        ),
        (
            "editor",
            &settings.editor.font_family,
            settings.editor.font_size,
        ),
    ] {
        if font.trim().is_empty() || font.len() > 256 || font.contains(['\0', '\n', '\r']) {
            reject(&format!("{field}.fontFamily"), "请输入有效的等宽字体名称。");
        }
        if !(10..=32).contains(&size) {
            reject(&format!("{field}.fontSize"), "字号必须介于 10 到 32。");
        }
    }
    if !matches!(settings.editor.tab_size, 2 | 4 | 8) {
        reject("editor.tabSize", "Tab 宽度只能选择 2、4 或 8。");
    }
    if !(1000..=50000).contains(&settings.terminal.scrollback_lines) {
        reject(
            "terminal.scrollbackLines",
            "回滚行数必须介于 1000 到 50000。",
        );
    }
    if settings.terminal.profiles.is_empty() || settings.terminal.profiles.len() > 32 {
        reject("terminal.profiles", "请保留 1 到 32 个终端配置。");
    }
    let mut ids = std::collections::HashSet::new();
    for (index, profile) in settings.terminal.profiles.iter().enumerate() {
        let field = format!("terminal.profiles.{index}");
        if profile.profile_id.is_empty()
            || profile.profile_id.len() > 128
            || !profile
                .profile_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !ids.insert(&profile.profile_id)
        {
            reject(&format!("{field}.profileId"), "终端配置标识无效或重复。");
        }
        if profile.name.trim().is_empty()
            || profile.name.len() > 128
            || profile.name.contains(['\0', '\r', '\n'])
        {
            reject(&format!("{field}.name"), "请输入有效的终端配置名称。");
        }
        if profile.executable_path.as_ref().is_some_and(|p| {
            p.trim().is_empty()
                || p.len() > 32768
                || p.contains('\0')
                || !std::path::Path::new(p).is_absolute()
        }) {
            reject(
                &format!("{field}.executablePath"),
                "请选择终端程序的绝对路径。",
            );
        }
        if profile.args.len() > 64
            || profile
                .args
                .iter()
                .any(|a| a.contains('\0') || a.len() > 16384)
            || profile.args.iter().map(String::len).sum::<usize>() > 65536
        {
            reject(&format!("{field}.args"), "启动参数超出范围或包含无效字符。");
        }
        if let TerminalDirectory::Fixed { path } = &profile.cwd {
            if path.trim().is_empty()
                || path.len() > 32768
                || path.contains('\0')
                || !std::path::Path::new(path).is_absolute()
            {
                reject(&format!("{field}.cwd"), "请选择有效的绝对目录路径。");
            }
        }
    }
    if !ids.contains(&settings.terminal.default_profile_id) {
        reject("terminal.defaultProfileId", "请选择存在的默认终端配置。");
    }
    if settings
        .git_path
        .as_ref()
        .is_some_and(|p| p.trim().is_empty() || p.len() > 32768 || p.contains('\0'))
    {
        reject("gitPath", "请选择有效的 Git 路径。");
    }
    errors
}

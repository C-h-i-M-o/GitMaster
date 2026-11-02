//! Codex 主题与外观模式；枚举使非法组合在 IPC 边界被拒绝。
use serde::{Deserialize, Serialize};

/// 用户的模式选择，系统模式不改写两个主题偏好。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    #[default]
    System,
    Light,
    Dark,
}

/// Codex 提供浅色变体的 14 个主题。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LightTheme {
    Absolutely,
    Catppuccin,
    Everforest,
    #[default]
    Github,
    Gruvbox,
    Linear,
    Notion,
    One,
    Raycast,
    RosePine,
    Solarized,
    Vercel,
    VscodePlus,
    Xcode,
}

/// 图示的 27 个深色主题。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DarkTheme {
    Absolutely,
    Ayu,
    Catppuccin,
    Dracula,
    Everforest,
    #[default]
    Github,
    Gruvbox,
    Linear,
    Lobster,
    Material,
    Matrix,
    Monokai,
    NightOwl,
    Nord,
    Notion,
    Og,
    One,
    Oscurange,
    Raycast,
    RosePine,
    Sentry,
    Solarized,
    Temple,
    TokyoNight,
    Vercel,
    VscodePlus,
    Xcode,
}

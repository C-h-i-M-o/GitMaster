//! 从 Git 的配置来源信息捕获可信认证设置，不将配置值写入错误或日志。
use super::super::{process::inspect_scoped_config, GitExecutable, OperationError};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Instant};

/// 来源校验后保留多值配置，credential.helper 的顺序与空值重置不能丢失。
pub(super) struct Entry {
    pub key: String,
    pub value: String,
}

/// 网络执行仅使用捕获的认证设置；摘要用于确认后失效检查。
pub(super) struct AuthPolicy {
    pub settings: Vec<(String, String)>,
    pub entries: Vec<Entry>,
    pub digest: [u8; 32],
}
impl AuthPolicy {
    /// Git 自己展开 include 并标明 scope，无法识别来源时拒绝执行。
    pub fn capture(
        git: &GitExecutable,
        root: &Path,
        deadline: Instant,
    ) -> Result<Self, OperationError> {
        let raw = inspect_scoped_config(git, root, deadline)?;
        Self::parse(&raw)
    }
    /// 配置记录必须按 scope/键值对完整解码，不通过有损转换接受命令内容。
    fn parse(raw: &[u8]) -> Result<Self, OperationError> {
        let fields: Vec<_> = raw.split(|b| *b == 0).collect();
        if fields.last() != Some(&&b""[..]) || (fields.len() - 1) % 2 != 0 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let mut entries = Vec::new();
        let mut settings = Vec::new();
        for pair in fields[..fields.len() - 1].chunks_exact(2) {
            let scope =
                std::str::from_utf8(pair[0]).map_err(|_| OperationError::new("PARSE_FAILED"))?;
            if !matches!(
                scope,
                "system" | "global" | "local" | "worktree" | "command"
            ) {
                return Err(OperationError::new("UNTRUSTED_AUTH_CONFIGURATION"));
            }
            let field =
                std::str::from_utf8(pair[1]).map_err(|_| OperationError::new("PARSE_FAILED"))?;
            let (key, value) = field.split_once('\n').unwrap_or((field, ""));
            let lowered = key.to_ascii_lowercase();
            let credential = lowered.starts_with("credential.") || lowered.starts_with("http.");
            let ssh_override = matches!(lowered.as_str(), "core.sshcommand" | "core.askpass")
                || lowered.starts_with("ssh.");
            if (credential || ssh_override) && !matches!(scope, "system" | "global") {
                return Err(OperationError::new("UNTRUSTED_AUTH_CONFIGURATION"));
            }
            // 固定 SSH 命令确保非交互和主机校验；定制 transport 需在外部配置成 agent/SSH Host。
            if ssh_override {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
            if credential {
                if lowered.ends_with(".sslverify")
                    && !matches!(
                        value.to_ascii_lowercase().as_str(),
                        "" | "true" | "yes" | "on" | "1"
                    )
                {
                    return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
                }
                settings.push((key.to_owned(), value.to_owned()));
            }
            entries.push(Entry {
                key: key.to_owned(),
                value: value.to_owned(),
            });
        }
        Ok(Self {
            settings,
            entries,
            digest: Sha256::digest(raw).into(),
        })
    }
    /// 重读源配置摘要，任何确认后的变化都要求重新准备。
    pub fn verify(
        &self,
        git: &GitExecutable,
        root: &Path,
        deadline: Instant,
    ) -> Result<(), OperationError> {
        let bytes = inspect_scoped_config(git, root, deadline)?;
        if <[u8; 32]>::from(Sha256::digest(bytes)) != self.digest {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(())
    }
    /// 返回某个配置的全部值，保留 Git 配置出现顺序。
    pub fn values(&self, key: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|entry| entry.key == key)
            .map(|entry| entry.value.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::tests::Fixture;
    use std::time::Duration;

    /// 多 helper 及空值重置原样保留；私有执行器不会再从仓库加载认证设置。
    #[test]
    fn trusted_helpers_preserve_order_and_local_overrides_are_rejected() {
        let policy = AuthPolicy::parse(b"system\0credential.helper\nfirst\0global\0credential.helper\n\0global\0credential.helper\nsecond\0local\0remote.origin.url\nhttps://example.invalid/a\0").unwrap();
        assert_eq!(
            policy.settings,
            [
                ("credential.helper".into(), "first".into()),
                ("credential.helper".into(), "".into()),
                ("credential.helper".into(), "second".into())
            ]
        );
        for key in [
            "credential.helper",
            "credential.https://example.invalid.helper",
            "http.extraheader",
            "core.sshcommand",
            "core.askpass",
            "ssh.variant",
        ] {
            for scope in ["local", "worktree", "command"] {
                let input = format!("{scope}\0{key}\nsecret\0");
                assert_eq!(
                    AuthPolicy::parse(input.as_bytes()).err().unwrap().code,
                    "UNTRUSTED_AUTH_CONFIGURATION"
                );
            }
        }
        assert!(
            AuthPolicy::parse(b"global\0http.https://example.invalid.sslverify\nfalse\0").is_err()
        );
        assert!(AuthPolicy::parse(b"unknown\0credential.helper\nsecret\0").is_err());
    }

    /// 使用真实 scope 输出发现仓库级 helper，包含它的文件不会被执行。
    #[test]
    fn actual_git_scope_rejects_repository_helper_and_changed_configuration() {
        let f = Fixture::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        let initial = AuthPolicy::capture(&f.git, &f.root, deadline).unwrap();
        assert!(initial.entries.iter().any(|entry| entry.key == "user.name"));
        f.command(&["config", "credential.helper", "!touch should-not-exist"]);
        assert_eq!(
            initial.verify(&f.git, &f.root, deadline).unwrap_err().code,
            "STALE_WRITE_PLAN"
        );
        assert_eq!(
            AuthPolicy::capture(&f.git, &f.root, deadline)
                .err()
                .unwrap()
                .code,
            "UNTRUSTED_AUTH_CONFIGURATION"
        );
        assert!(!f.root.join("should-not-exist").exists());
    }
}

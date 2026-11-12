//! AI 终端权限预检与用户确认修复；该能力不进入模型工具注册表。
use gitmaster_core::git::OperationError;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

/// 修复票据只在当前窗口、当前项目有效，消费后不能重放。
#[derive(Default)]
pub struct SandboxRepairs(Mutex<HashMap<String, Repair>>);
struct Repair {
    owner: String,
    root: PathBuf,
    output: PathBuf,
    created: Instant,
}
/// 只读检查同时报告阻塞步骤，不能把文件可写等同于可配置沙箱。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxStatus {
    pub ready: bool,
    pub root: PathBuf,
    pub code: String,
    pub operation: String,
    pub win32_code: Option<u32>,
    pub write_dac: bool,
    pub write_owner: bool,
    pub reparse: bool,
    pub shell_available: bool,
    pub owner: String,
    pub owned_by_current_user: bool,
}

/// 拒绝根目录及含重解析点的目录链，防止固定授权目标被链接扩大。
fn validate_root(root: &Path) -> Result<PathBuf, OperationError> {
    let canonical =
        std::fs::canonicalize(root).map_err(|_| OperationError::new("AI_SESSION_PROJECT"))?;
    if root.parent().is_none() || canonical.parent().is_none() {
        return Err(OperationError::new("AI_SANDBOX_ROOT"));
    }
    for path in root.ancestors() {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| OperationError::new("AI_SANDBOX_PATH"))?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(OperationError::new("AI_SANDBOX_REPARSE"));
            }
        }
        #[cfg(not(windows))]
        if metadata.file_type().is_symlink() {
            return Err(OperationError::new("AI_SANDBOX_REPARSE"));
        }
    }
    Ok(canonical)
}

/// 以所需权限打开目录而不写入内容或安全描述符，使用当前令牌的有效访问检查。
#[cfg(windows)]
fn access(root: &Path, rights: u32) -> Result<(), u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, OPEN_EXISTING},
    };
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            rights,
            7,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(unsafe { GetLastError() })
    } else {
        unsafe {
            CloseHandle(handle);
        }
        Ok(())
    }
}

/// 检查没有副作用；成功结果仍不能替代后端真实创建沙箱时的安全校验。
pub fn inspect(root: &Path) -> Result<SandboxStatus, OperationError> {
    let root = validate_root(root)?;
    #[cfg(windows)]
    let (dac, owner) = (access(&root, 0x40000), access(&root, 0x80000));
    #[cfg(not(windows))]
    let (dac, owner): (Result<(), u32>, Result<(), u32>) = (Ok(()), Ok(()));
    let mut command = std::process::Command::new(if cfg!(windows) { "pwsh" } else { "bash" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.args(if cfg!(windows) { vec!["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", "$a=Get-Acl -LiteralPath $env:GITMASTER_AI_INSPECT_ROOT; $s=$a.GetOwner([System.Security.Principal.SecurityIdentifier]); @{owner=$s.Value;ownedByCurrentUser=($s.Value -eq [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value)} | ConvertTo-Json -Compress"] } else {vec!["--version"]});
    command.env("GITMASTER_AI_INSPECT_ROOT", dunce::simplified(&root));
    let result = command.output();
    let shell_available = result.as_ref().map(|r| r.status.success()).unwrap_or(false);
    let identity: Value = result
        .as_ref()
        .ok()
        .and_then(|r| serde_json::from_slice(&r.stdout).ok())
        .unwrap_or(json!({}));
    let owned_by_current_user = if cfg!(windows) {
        identity["ownedByCurrentUser"].as_bool().unwrap_or(false)
    } else {
        true
    };
    let ready = dac.is_ok() && owner.is_ok() && shell_available && owned_by_current_user;
    Ok(SandboxStatus {
        ready,
        root,
        code: if ready {
            "OK"
        } else if !shell_available {
            "AI_SHELL_UNAVAILABLE"
        } else {
            "AI_SANDBOX_ACL"
        }
        .into(),
        operation: if !shell_available {
            "shell"
        } else if !owned_by_current_user {
            "owner"
        } else if dac.is_err() {
            "WRITE_DAC"
        } else if owner.is_err() {
            "WRITE_OWNER"
        } else {
            "ready"
        }
        .into(),
        win32_code: dac.err().or(owner.err()),
        write_dac: dac.is_ok(),
        write_owner: owner.is_ok(),
        reparse: false,
        shell_available,
        owner: identity["owner"].as_str().unwrap_or("").into(),
        owned_by_current_user,
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    /// 在独立临时目录复现“当前用户拥有 + Modify + 无 WRITE_OWNER”，不接触用户仓库。
    #[test]
    fn owned_modify_acl_requires_explicit_repair() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("fixture.txt"), "安全回归").unwrap();
        let before = inspect(&root).unwrap();
        assert!(before.ready, "临时目录控制组必须可配置沙箱");
        let output=std::process::Command::new("pwsh").args(["-NoProfile","-NonInteractive","-Command","$p=$env:GITMASTER_AI_TEST_ROOT; $a=Get-Acl -LiteralPath $p; $a.SetAccessRuleProtection($true,$false); foreach($r in @($a.Access)){$a.RemoveAccessRuleSpecific($r)}; $s=[System.Security.Principal.WindowsIdentity]::GetCurrent().User; $r=[System.Security.AccessControl.FileSystemAccessRule]::new($s,'Modify','ContainerInherit,ObjectInherit','None','Allow'); $a.AddAccessRule($r); Set-Acl -LiteralPath $p -AclObject $a"]).env("GITMASTER_AI_TEST_ROOT",&root).output().unwrap();
        assert!(output.status.success(), "临时权限夹具创建失败");
        let blocked = inspect(&root).unwrap();
        assert!(!blocked.ready);
        assert!(blocked.owned_by_current_user);
        assert!(blocked.write_dac);
        assert!(!blocked.write_owner);
        assert_eq!(blocked.win32_code, Some(5));
        let repairs = SandboxRepairs::default();
        let backups = directory.path().join("backups");
        let preview = repairs.prepare("fixture", &root, &backups).unwrap();
        assert!(!inspect(&root).unwrap().ready, "准备操作不能修改权限");
        let id = preview["confirmationId"].as_str().unwrap();
        let result = repairs.execute("fixture", &root, id).unwrap();
        assert_eq!(result["ok"], true, "{result}");
        assert!(inspect(&root).unwrap().ready);
        assert!(
            repairs.execute("fixture", &root, id).is_err(),
            "禁止票据重放"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("fixture.txt")).unwrap(),
            "安全回归"
        );
    }
}

impl SandboxRepairs {
    /// 准备确切目标与备份位置，此步骤不修改项目权限。
    pub fn prepare(
        &self,
        owner: &str,
        root: &Path,
        app_data: &Path,
    ) -> Result<Value, OperationError> {
        let root = validate_root(root)?;
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| OperationError::new("AI_CONFIRMATION_FAILED"))?;
        let id = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let output = app_data.join("ai/acl-recovery").join(&id);
        let result = json!({"confirmationId":id,"root":root,"backupPath":output,"detail":"将按上游脚本检查当前项目目录树（最多 2000 个目录），补充当前用户缺少的完全控制授权，并移除冲突的显式 AppContainer 包授权。保留所有者、继承和拒绝项，不提权。授权常驻；备份仅覆盖本次 DACL 改动，不包含历史完整性标签恢复。"});
        let mut pending = self
            .0
            .lock()
            .map_err(|_| OperationError::new("AI_RUNTIME_FAILED"))?;
        pending.retain(|_, item| item.created.elapsed() < Duration::from_secs(300));
        pending.insert(
            id,
            Repair {
                owner: owner.into(),
                root,
                output,
                created: Instant::now(),
            },
        );
        Ok(result)
    }
    /// 原生确认只消费一次票据，执行前再次核对窗口与当前项目，脚本和参数均固定。
    pub fn execute(&self, owner: &str, root: &Path, id: &str) -> Result<Value, OperationError> {
        let repair = self
            .0
            .lock()
            .map_err(|_| OperationError::new("AI_RUNTIME_FAILED"))?
            .remove(id)
            .ok_or_else(|| OperationError::new("AI_CONFIRMATION_EXPIRED"))?;
        if repair.owner != owner
            || repair.root != validate_root(root)?
            || repair.created.elapsed() > Duration::from_secs(300)
        {
            return Err(OperationError::new("AI_CONFIRMATION_EXPIRED"));
        }
        if !cfg!(windows) {
            return Err(OperationError::new("AI_PLATFORM_UNSUPPORTED"));
        }
        std::fs::create_dir_all(&repair.output)
            .map_err(|_| OperationError::new("AI_SANDBOX_BACKUP"))?;
        let script = repair.output.join("repair.ps1");
        std::fs::write(
            &script,
            include_bytes!("../../services/agent-host/assets/windows-acl/repair.ps1"),
        )
        .map_err(|_| OperationError::new("AI_SANDBOX_BACKUP"))?;
        let mut command = std::process::Command::new("pwsh");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let result = command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
            .arg(script)
            .arg("-Path")
            .arg(dunce::simplified(&repair.root))
            .arg("-AllowRoot")
            .arg(dunce::simplified(&repair.root))
            .arg("-Out")
            .arg(&repair.output)
            .output()
            .map_err(|_| OperationError::new("AI_SHELL_UNAVAILABLE"))?;
        let status = inspect(&repair.root)?;
        Ok(
            json!({"ok":result.status.success() && status.ready,"status":status,"backupPath":repair.output,"detail":String::from_utf8_lossy(&result.stdout).chars().rev().take(12000).collect::<Vec<_>>().into_iter().rev().collect::<String>(),"error":String::from_utf8_lossy(&result.stderr).chars().take(2000).collect::<String>()}),
        )
    }
}

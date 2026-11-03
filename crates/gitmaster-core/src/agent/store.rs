use super::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::SystemTime,
};

const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_EVENT: usize = 8 * 1024 * 1024;

#[cfg(test)]
thread_local! { static SCANNED_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

#[cfg(test)]
mod review_tests {
    use super::*;

    /// 连续追加的历史扫描总量不得随事件数二次增长；用字节计数避免机器耗时噪声。
    #[test]
    fn append_does_not_rescan_unchanged_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentStore::open(dir.path().into()).unwrap();
        store
            .save_profile(ProviderProfile {
                id: "aa".into(),
                name: "测试".into(),
                protocol: ProviderProtocol::Responses,
                base_url: "http://127.0.0.1:9999".into(),
                model: "test".into(),
                credential_id: "bb".into(),
            })
            .unwrap();
        let chat = store.create_chat("aa", None).unwrap();
        SCANNED_BYTES.with(|value| value.set(0));
        for _ in 0..24 {
            store
                .append(
                    &chat.id,
                    "ab",
                    EventKind::UserMessage {
                        text: "中".repeat(4096),
                    },
                )
                .unwrap();
        }
        let size = fs::metadata(store.chat_dir(&chat.id).unwrap().join("events.jsonl"))
            .unwrap()
            .len() as usize;
        assert!(
            SCANNED_BYTES.with(|value| value.get()) <= size * 2,
            "正常追加不应反复读取历史"
        );
        assert_eq!(store.read_chat(&chat.id).unwrap().events.len(), 24);
    }
}

/// 持有应用私有 Agent 目录，所有更改在进程锁和目录锁内执行。
pub struct AgentStore {
    root: PathBuf,
    gate: Mutex<HashMap<String, EventTail>>,
}

/// 只缓存已校验的尾部；外部更改通过文件长度及时间戳使缓存失效。
#[derive(Clone)]
struct EventTail {
    sequence: u64,
    complete: u64,
    stamp: Option<EventStamp>,
}

#[derive(Clone, PartialEq, Eq)]
struct EventStamp {
    len: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
}

/// 不跟随符号链接；修改或替换文件后必须重新验证完整历史。
fn event_stamp(path: &Path) -> AgentResult<Option<EventStamp>> {
    regular_or_missing(path)?;
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(EventStamp {
            len: metadata.len(),
            modified: metadata.modified().map_err(AgentError::storage)?,
            created: metadata.created().ok(),
        })),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AgentError::storage(error)),
    }
}

/// 锁文件关闭时自动释放操作级系统锁。
struct StoreGuard<'a> {
    process: MutexGuard<'a, HashMap<String, EventTail>>,
    _file: File,
}

impl AgentStore {
    /// 创建专用目录；不读取密钥，也不跟随内部符号链接。
    pub fn open(root: PathBuf) -> AgentResult<Self> {
        fs::create_dir_all(&root).map_err(AgentError::storage)?;
        directory(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(AgentError::storage)?;
        }
        let store = Self {
            root,
            gate: Mutex::new(HashMap::new()),
        };
        let _guard = store.lock()?;
        let chats = store.root.join("chats");
        fs::create_dir_all(&chats).map_err(AgentError::storage)?;
        directory(&chats)?;
        drop(_guard);
        Ok(store)
    }
    /// 同进程串行化，并阻止多个程序实例同时修改会话文件。
    fn lock(&self) -> AgentResult<StoreGuard<'_>> {
        let process = self.gate.lock().map_err(AgentError::storage)?;
        directory(&self.root)?;
        let path = self.root.join(".lock");
        regular_or_missing(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(AgentError::storage)?;
        file.try_lock().map_err(|_| {
            AgentError::new(
                "AGENT_STORAGE_BUSY",
                "另一个 GitMaster 正在使用 AI 数据，请稍后重试。",
            )
        })?;
        Ok(StoreGuard {
            process,
            _file: file,
        })
    }
    /// 运行控制器独占整个生命周期，避免另一实例把在途操作误判为崩溃恢复。
    pub(crate) fn runtime_lease(&self) -> AgentResult<File> {
        let _guard = self.lock()?;
        let path = self.root.join(".runtime.lock");
        regular_or_missing(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(AgentError::storage)?;
        file.try_lock().map_err(|_| {
            AgentError::new(
                "AGENT_RUNTIME_BUSY",
                "另一个 GitMaster 正在运行 AI，请关闭该实例后重试。",
            )
        })?;
        Ok(file)
    }
    /// 返回配置快照；未来版本或坏数据不会被默认值覆盖。
    pub fn settings(&self) -> AgentResult<AgentSettings> {
        let _guard = self.lock()?;
        self.read_settings()
    }
    /// 在持锁期间校验磁盘配置的版本、引用和全部已保存配置。
    fn read_settings(&self) -> AgentResult<AgentSettings> {
        let path = self.root.join("settings.json");
        regular_or_missing(&path)?;
        if !path.exists() {
            return Ok(AgentSettings::default());
        }
        let settings: AgentSettings = read_json(&path)?;
        if settings.version != 1 || settings.profiles.len() > 32 {
            return Err(AgentError::corrupt());
        }
        let mut ids = std::collections::HashSet::new();
        for profile in &settings.profiles {
            profile.validate().map_err(|_| AgentError::corrupt())?;
            if !ids.insert(&profile.id) {
                return Err(AgentError::corrupt());
            }
        }
        if settings
            .active_profile_id
            .as_ref()
            .is_some_and(|id| !ids.contains(id))
        {
            return Err(AgentError::corrupt());
        }
        Ok(settings)
    }
    /// 原子更新单个配置，不影响其他配置；首次保存自动选中。
    pub fn save_profile(&self, profile: ProviderProfile) -> AgentResult<AgentSettings> {
        profile.validate()?;
        let _guard = self.lock()?;
        let mut settings = self.read_settings()?;
        if let Some(existing) = settings.profiles.iter_mut().find(|p| p.id == profile.id) {
            *existing = profile.clone();
        } else {
            if settings.profiles.len() >= 32 {
                return Err(AgentError::new(
                    "AGENT_PROFILE_LIMIT",
                    "最多保存 32 个 API 配置。",
                ));
            }
            settings.profiles.push(profile.clone());
        }
        if settings.active_profile_id.is_none() {
            settings.active_profile_id = Some(profile.id);
        }
        atomic_json(&self.root.join("settings.json"), &settings)?;
        Ok(settings)
    }
    /// 删除配置引用；凭据删除由平台层在同一配置操作内完成。
    pub fn delete_profile(&self, id: &str) -> AgentResult<AgentSettings> {
        let _guard = self.lock()?;
        let mut settings = self.read_settings()?;
        settings.profiles.retain(|p| p.id != id);
        if settings.active_profile_id.as_deref() == Some(id) {
            settings.active_profile_id = settings.profiles.first().map(|p| p.id.clone());
        }
        atomic_json(&self.root.join("settings.json"), &settings)?;
        Ok(settings)
    }
    /// 只允许选择存在的配置；聊天在下一轮使用该选择。
    pub fn select_profile(&self, id: &str) -> AgentResult<AgentSettings> {
        let _guard = self.lock()?;
        let mut settings = self.read_settings()?;
        if !settings.profiles.iter().any(|p| p.id == id) {
            return Err(AgentError::new(
                "AGENT_PROFILE_MISSING",
                "API 配置不存在，请在设置中重新选择。",
            ));
        }
        settings.active_profile_id = Some(id.into());
        atomic_json(&self.root.join("settings.json"), &settings)?;
        Ok(settings)
    }
    /// 保存下轮生效的默认权限，不改变正在执行的运行快照。
    pub fn set_permission(&self, mode: PermissionMode) -> AgentResult<AgentSettings> {
        let _guard = self.lock()?;
        let mut settings = self.read_settings()?;
        settings.default_permission_mode = mode;
        atomic_json(&self.root.join("settings.json"), &settings)?;
        Ok(settings)
    }
    /// 建立独立聊天目录和元数据，不创建云端会话。
    pub fn create_chat(
        &self,
        profile_id: &str,
        repository_path: Option<String>,
    ) -> AgentResult<ChatSession> {
        let _guard = self.lock()?;
        let settings = self.read_settings()?;
        if !settings.profiles.iter().any(|p| p.id == profile_id) {
            return Err(AgentError::new(
                "AGENT_PROFILE_MISSING",
                "请先完成 API 配置。",
            ));
        }
        let now = timestamp();
        let chat = ChatSession {
            version: 1,
            id: new_id(),
            title: "新对话".into(),
            repository_path,
            profile_id: profile_id.into(),
            permission_mode: settings.default_permission_mode,
            archived: false,
            created_at: now,
            updated_at: now,
        };
        directory(&self.root.join("chats"))?;
        let dir = self.root.join("chats").join(&chat.id);
        fs::create_dir(&dir).map_err(AgentError::storage)?;
        atomic_json(&dir.join("session.json"), &chat)?;
        Ok(chat)
    }
    /// 验证聊天目录，禁止前端传入路径或内部符号链接。
    fn chat_dir(&self, id: &str) -> AgentResult<PathBuf> {
        valid_id(id)?;
        directory(&self.root.join("chats"))?;
        let dir = self.root.join("chats").join(id);
        directory(&dir)?;
        Ok(dir)
    }
    /// 返回可用聊天和单独的损坏记录列表。
    pub fn list_chats(&self) -> AgentResult<ChatList> {
        let _guard = self.lock()?;
        self.list_unlocked()
    }
    /// 在目录锁内读取轻量元数据，不加载所有模型上下文。
    fn list_unlocked(&self) -> AgentResult<ChatList> {
        directory(&self.root.join("chats"))?;
        let mut list = ChatList {
            chats: vec![],
            unavailable: vec![],
        };
        for entry in fs::read_dir(self.root.join("chats")).map_err(AgentError::storage)? {
            let entry = entry.map_err(AgentError::storage)?;
            let id = entry.file_name().to_string_lossy().into_owned();
            match self.session(&id) {
                Ok(chat) => list.chats.push(chat),
                Err(_) => list.unavailable.push(id),
            }
        }
        list.chats.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(list)
    }
    /// 校验元数据版本及 ID 与目录的一致性。
    fn session(&self, id: &str) -> AgentResult<ChatSession> {
        let chat: ChatSession = read_json(&self.chat_dir(id)?.join("session.json"))?;
        if chat.version != 1 || chat.id != id {
            return Err(AgentError::corrupt());
        }
        Ok(chat)
    }
    /// 恢复所有完整事件，未换行的中断尾部不算已提交事件。
    pub fn read_chat(&self, id: &str) -> AgentResult<ChatRecord> {
        let _guard = self.lock()?;
        let session = self.session(id)?;
        let (events, _) = self.events(id)?;
        Ok(ChatRecord { session, events })
    }
    /// 读取 JSONL 并确认序列连续；只容忍未完成的最后一条。
    fn events(&self, id: &str) -> AgentResult<(Vec<AgentEvent>, u64)> {
        let path = self.chat_dir(id)?.join("events.jsonl");
        regular_or_missing(&path)?;
        if !path.exists() {
            return Ok((vec![], 0));
        }
        let bytes = read_bounded(&path)?;
        #[cfg(test)]
        SCANNED_BYTES.with(|count| count.set(count.get() + bytes.len()));
        let complete = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        let mut events = vec![];
        for line in bytes[..complete]
            .split(|b| *b == b'\n')
            .filter(|b| !b.is_empty())
        {
            let event: AgentEvent =
                serde_json::from_slice(line).map_err(|_| AgentError::corrupt())?;
            if event.version != 1
                || event.session_id != id
                || event.sequence != events.len() as u64 + 1
            {
                return Err(AgentError::corrupt());
            }
            events.push(event);
        }
        Ok((events, complete as u64))
    }
    /// 追加前修复未落稳的尾记录，sync 成功后才交付事件。
    pub fn append(&self, id: &str, run_id: &str, payload: EventKind) -> AgentResult<AgentEvent> {
        valid_id(run_id)?;
        let mut guard = self.lock()?;
        let mut session = self.session(id)?;
        let path = self.chat_dir(id)?.join("events.jsonl");
        let stamp = event_stamp(&path)?;
        // 先取出缓存，任何扫描、写入或同步失败都不保留可能已过期的尾部。
        let tail = match guard.process.remove(id).filter(|tail| tail.stamp == stamp) {
            Some(tail) => tail,
            None => {
                let (events, complete) = self.events(id)?;
                EventTail {
                    sequence: events.len() as u64,
                    complete,
                    stamp,
                }
            }
        };
        let complete = tail.complete;
        let event = AgentEvent {
            version: 1,
            sequence: tail.sequence + 1,
            session_id: id.into(),
            run_id: run_id.into(),
            timestamp: timestamp(),
            payload,
        };
        let mut bytes = serde_json::to_vec(&event).map_err(AgentError::storage)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_EVENT || complete + bytes.len() as u64 > MAX_FILE {
            return Err(AgentError::new(
                "AGENT_CHAT_LIMIT",
                "当前对话过长，请新建对话后继续。",
            ));
        }
        regular_or_missing(&path)?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(AgentError::storage)?;
        file.set_len(complete).map_err(AgentError::storage)?;
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::End(0)).map_err(AgentError::storage)?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(AgentError::storage)?;
        // Windows 在关闭写句柄后才保证文件时间更新，不能缓存旧修改时间。
        drop(file);
        session.updated_at = event.timestamp;
        atomic_json(&self.chat_dir(id)?.join("session.json"), &session)?;
        guard.process.insert(
            id.into(),
            EventTail {
                sequence: event.sequence,
                complete: complete + bytes.len() as u64,
                stamp: event_stamp(&path)?,
            },
        );
        Ok(event)
    }
    /// 重命名或归档不复制聊天内容，磁盘统计随元数据分类变化。
    pub fn update_chat(&self, id: &str, title: &str, archived: bool) -> AgentResult<ChatSession> {
        if title.trim().is_empty() || title.chars().count() > 120 {
            return Err(AgentError::new(
                "AGENT_INVALID_TITLE",
                "聊天名称须为 1～120 个字符。",
            ));
        }
        let _guard = self.lock()?;
        let mut chat = self.session(id)?;
        chat.title = title.trim().into();
        chat.archived = archived;
        chat.updated_at = timestamp();
        atomic_json(&self.chat_dir(id)?.join("session.json"), &chat)?;
        Ok(chat)
    }
    /// 删除只能作用于已验证的不透明聊天目录。
    pub fn delete_chat(&self, id: &str) -> AgentResult<()> {
        let mut guard = self.lock()?;
        let dir = self.chat_dir(id)?;
        guard.process.remove(id);
        fs::remove_dir_all(dir).map_err(AgentError::storage)
    }
    /// 计算真实文件字节数，不跟随符号链接或统计外部仓库。
    pub fn usage(&self) -> AgentResult<AgentUsage> {
        let _guard = self.lock()?;
        let list = self.list_unlocked()?;
        let mut usage = AgentUsage {
            location: self.root.to_string_lossy().into_owned(),
            ..Default::default()
        };
        for chat in list.chats {
            let size = directory_bytes(&self.chat_dir(&chat.id)?)?;
            if chat.archived {
                usage.archived_bytes += size;
            } else {
                usage.chat_bytes += size;
            }
        }
        let total = directory_bytes(&self.root)?;
        usage.other_bytes = total.saturating_sub(usage.chat_bytes + usage.archived_bytes);
        Ok(usage)
    }
}

/// 确认目录本体不是链接，拒绝链接形式的会话和数据目录。
fn directory(path: &Path) -> AgentResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(AgentError::storage)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AgentError::storage("目录无效"));
    }
    Ok(())
}
/// 文件必须是普通文件或尚不存在；设备、目录、链接均拒绝。
fn regular_or_missing(path: &Path) -> AgentResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(AgentError::storage("文件无效")),
    }
}
/// 给配置与会话读取设置内存上限，不读取设备或不受限输入。
fn read_bounded(path: &Path) -> AgentResult<Vec<u8>> {
    use std::io::Read;
    regular_or_missing(path)?;
    let file = File::open(path).map_err(AgentError::storage)?;
    if file.metadata().map_err(AgentError::storage)?.len() > MAX_FILE {
        return Err(AgentError::corrupt());
    }
    let mut bytes = vec![];
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(AgentError::storage)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(AgentError::corrupt());
    }
    Ok(bytes)
}
/// 严格读取结构化数据，损坏时保留原文件。
fn read_json<T: DeserializeOwned>(path: &Path) -> AgentResult<T> {
    serde_json::from_slice(&read_bounded(path)?).map_err(|_| AgentError::corrupt())
}
/// 在同目录写临时文件并原子替换，避免半份配置。
fn atomic_json(path: &Path, value: &impl Serialize) -> AgentResult<()> {
    regular_or_missing(path)?;
    let parent = path.parent().ok_or_else(AgentError::corrupt)?;
    directory(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(AgentError::storage)?;
    serde_json::to_writer(&mut file, value).map_err(AgentError::storage)?;
    file.as_file().sync_all().map_err(AgentError::storage)?;
    file.persist(path).map_err(AgentError::storage)?;
    Ok(())
}
/// 遍历应用自有目录；符号链接占用不计入，以免越界或重复累计。
pub fn directory_bytes(path: &Path) -> AgentResult<u64> {
    let mut total = 0;
    for entry in fs::read_dir(path).map_err(AgentError::storage)? {
        let entry = entry.map_err(AgentError::storage)?;
        let kind = entry.file_type().map_err(AgentError::storage)?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            total += directory_bytes(&entry.path())?;
        } else if kind.is_file() {
            total += entry.metadata().map_err(AgentError::storage)?.len();
        }
    }
    Ok(total)
}

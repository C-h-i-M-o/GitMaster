//! 内存有界、磁盘无总量配额的展示缓存；不保存工作区正文、草稿、一次性计划或会话授权。

use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MEMORY_LIMIT: usize = 128 * 1024 * 1024;
const BLOCK_LIMIT: usize = 8 * 1024 * 1024;
const MANIFEST_LIMIT: usize = 4 * 1024 * 1024;
const MANIFEST: &str = "manifest.json";

/// 用量来自真实文件，不把读取失败伪装成零占用。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheUsage {
    pub disk_bytes: u64,
    pub content_bytes: u64,
}

/// 平台无关的两级内容缓存，调用方必须把仓库身份和查询语义纳入键。
pub struct ContentCache {
    disk: DiskStore,
    memory: Mutex<Memory>,
}

#[derive(Default)]
struct Memory {
    entries: BTreeMap<String, (Vec<u8>, u64)>,
    bytes: usize,
    tick: u64,
    limit: usize,
}

impl ContentCache {
    /// 在平台提供的专用缓存目录创建始终启用的缓存，不访问任何仓库。
    pub fn new(root: &Path) -> io::Result<Self> {
        Self::with_memory_limit(root, MEMORY_LIMIT)
    }

    /// 内存预算可缩小以验证淘汰；磁盘没有总量配额。
    fn with_memory_limit(root: &Path, memory_limit: usize) -> io::Result<Self> {
        std::fs::create_dir_all(root)?;
        if !std::fs::symlink_metadata(root)?.file_type().is_dir() {
            return Err(io::Error::other("缓存目录不是普通目录"));
        }
        Ok(Self {
            disk: DiskStore {
                dir: Dir::open_ambient_dir(root, cap_std::ambient_authority())?,
            },
            memory: Mutex::new(Memory {
                limit: memory_limit,
                ..Memory::default()
            }),
        })
    }

    /// 读取失败、锁竞争或坏块均返回未命中，由调用方执行真实查询。
    pub fn get(&self, repository_key: &str, content_key: &str) -> Option<Vec<u8>> {
        let key = key_digest(repository_key, content_key);
        if let Ok(mut memory) = self.memory.try_lock() {
            memory.tick = memory.tick.saturating_add(1);
            let tick = memory.tick;
            if let Some((bytes, used)) = memory.entries.get_mut(&key) {
                *used = tick;
                return Some(bytes.clone());
            }
        }
        let value = self.disk.get(&key).ok().flatten()?;
        if let Ok(mut memory) = self.memory.try_lock() {
            memory.insert(key, &value);
        }
        Some(value)
    }

    /// 缓存写入是尽力而为；失败不改变调用方已经读取到的真实结果。
    pub fn put(&self, repository_key: &str, content_key: &str, value: &[u8]) -> bool {
        if value.len() > BLOCK_LIMIT {
            return false;
        }
        let key = key_digest(repository_key, content_key);
        let memory_saved = self
            .memory
            .try_lock()
            .map(|mut memory| memory.insert(key.clone(), value))
            .unwrap_or(false);
        let disk_saved = self.disk.put(&key, value).is_ok();
        memory_saved || disk_saved
    }

    /// 扫描真实磁盘占用和数据块占用，不把读取失败伪装成零。
    pub fn usage(&self) -> io::Result<CacheUsage> {
        let _lock = self.disk.lock()?;
        let (disk_bytes, content_bytes) = self.disk.bytes()?;
        Ok(CacheUsage {
            disk_bytes,
            content_bytes,
        })
    }

    /// 只清除专用目录中本格式生成的块与清单，保留锁文件和不认识的文件。
    pub fn clear(&self) -> io::Result<()> {
        let _lock = self.disk.lock()?;
        let mut memory = self.memory.try_lock().map_err(|_| busy())?;
        for entry in self.disk.dir.entries()? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if owned_file(name) {
                self.disk.dir.remove_file(name)?;
            }
        }
        memory.entries.clear();
        memory.bytes = 0;
        Ok(())
    }
}

impl Memory {
    /// 按字节预算淘汰最久未使用条目，不持有超预算的新块。
    fn insert(&mut self, key: String, bytes: &[u8]) -> bool {
        if bytes.len() > self.limit || self.limit == 0 {
            return false;
        }
        if let Some((old, _)) = self.entries.remove(&key) {
            self.bytes -= old.len();
        }
        while self.bytes + bytes.len() > self.limit {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, tick))| tick)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some((old, _)) = self.entries.remove(&oldest) {
                self.bytes -= old.len();
            }
        }
        self.tick = self.tick.saturating_add(1);
        self.bytes += bytes.len();
        self.entries.insert(key, (bytes.to_vec(), self.tick));
        true
    }
}

struct DiskStore {
    dir: Dir,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    entries: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    bytes: u64,
    sha256: String,
    used: u64,
}

impl Default for Manifest {
    /// 未知格式或损坏清单回退到空索引，旧块稍后作为孤块清理。
    fn default() -> Self {
        Self {
            version: 1,
            entries: BTreeMap::new(),
        }
    }
}

impl DiskStore {
    /// OS 锁随文件句柄释放，进程退出不会遗留永久锁定状态。
    fn lock(&self) -> io::Result<std::fs::File> {
        let file = self
            .dir
            .open_with(
                ".lock",
                OpenOptions::new().read(true).write(true).create(true),
            )?
            .into_std();
        file.try_lock().map_err(|_| busy())?;
        Ok(file)
    }

    /// 真实目录占用包含清单、锁及中断留下的临时文件，特殊目录项不参与缓存写入。
    fn bytes(&self) -> io::Result<(u64, u64)> {
        let mut total = 0u64;
        let mut content = 0u64;
        for entry in self.dir.entries()? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(io::Error::other("缓存含非普通文件"));
            }
            let size = entry.metadata()?.len();
            let name = entry.file_name();
            if name
                .to_str()
                .and_then(|name| name.strip_suffix(".block"))
                .is_some_and(valid_hash)
            {
                content = content
                    .checked_add(size)
                    .ok_or_else(|| io::Error::other("缓存大小超限"))?;
            }
            total = total
                .checked_add(size)
                .ok_or_else(|| io::Error::other("缓存大小超限"))?;
        }
        Ok((total, content))
    }

    /// 读取有界清单并验证全部文件名和大小，缓存内容不能指定目录或执行对象。
    fn manifest(&self) -> Manifest {
        let result = self
            .read(MANIFEST, MANIFEST_LIMIT)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok());
        result
            .filter(|manifest| {
                manifest.version == 1
                    && manifest.entries.iter().all(|(key, entry)| {
                        valid_hash(key)
                            && valid_hash(&entry.sha256)
                            && entry.bytes <= BLOCK_LIMIT as u64
                    })
            })
            .unwrap_or_default()
    }

    /// 即使文件在读取中增长也不分配无界内存，cap-std 限制目录逃逸。
    fn read(&self, name: &str, limit: usize) -> io::Result<Vec<u8>> {
        if !self.dir.symlink_metadata(name)?.file_type().is_file() {
            return Err(io::Error::other("缓存块不是普通文件"));
        }
        let file = self.dir.open(name)?;
        if file.metadata()?.len() > limit as u64 {
            return Err(io::Error::other("缓存块超限"));
        }
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            return Err(io::Error::other("缓存块超限"));
        }
        Ok(bytes)
    }

    /// 校验和失败只丢弃缓存命中，不向调用方提供损坏正文。
    fn get(&self, key: &str) -> io::Result<Option<Vec<u8>>> {
        let _lock = self.lock()?;
        let mut manifest = self.manifest();
        let Some(entry) = manifest.entries.get_mut(key) else {
            return Ok(None);
        };
        let value = match self.read(&block_name(key), BLOCK_LIMIT) {
            Ok(value) => value,
            Err(_) => {
                manifest.entries.remove(key);
                let _ = self.remove_block(key);
                let _ = self.save_manifest(&manifest);
                return Ok(None);
            }
        };
        if value.len() as u64 != entry.bytes || digest(&value) != entry.sha256 {
            manifest.entries.remove(key);
            let _ = self.remove_block(key);
            let _ = self.save_manifest(&manifest);
            return Ok(None);
        }
        Ok(Some(value))
    }

    /// 写入新块并原子更新索引；不因磁盘总量淘汰已有内容。
    fn put(&self, key: &str, value: &[u8]) -> io::Result<()> {
        let _lock = self.lock()?;
        let mut manifest = self.manifest();
        for entry in self.dir.entries()? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name == "block.tmp"
                || name == "manifest.tmp"
                || name
                    .strip_suffix(".block")
                    .is_some_and(|key| valid_hash(key) && !manifest.entries.contains_key(key))
            {
                self.dir.remove_file(name)?;
            }
        }
        if manifest.entries.remove(key).is_some() {
            self.remove_block(key)?;
        }
        manifest.entries.insert(
            key.into(),
            Entry {
                bytes: value.len() as u64,
                sha256: digest(value),
                used: timestamp(),
            },
        );
        // 写块前检查清单可编码；单次内存解析保护与磁盘总容量无关。
        encode(&manifest)?;
        self.atomic_write("block.tmp", &block_name(key), value)?;
        self.save_manifest(&manifest)
    }

    /// 索引引用缺失块等价于未命中，清理时允许已被系统移除。
    fn remove_block(&self, key: &str) -> io::Result<()> {
        match self.dir.remove_file(block_name(key)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    /// 清单采用同目录临时文件替换，实际空间不足时由文件系统报告错误。
    fn save_manifest(&self, manifest: &Manifest) -> io::Result<()> {
        let bytes = encode(manifest)?;
        self.atomic_write("manifest.tmp", MANIFEST, &bytes)
    }

    /// 发布失败清理本次临时文件，已发布的孤块仍计入后续真实占用。
    fn atomic_write(&self, temporary: &str, name: &str, bytes: &[u8]) -> io::Result<()> {
        let result = (|| {
            let mut file = self
                .dir
                .open_with(temporary, OpenOptions::new().write(true).create_new(true))?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            self.dir.rename(temporary, &self.dir, name)
        })();
        if result.is_err() {
            let _ = self.dir.remove_file(temporary);
        }
        result
    }
}

/// 键采用长度前缀隔离仓库和内容语义，清单不记录原始项目路径。
fn key_digest(scope: &str, key: &str) -> String {
    let mut hash = Sha256::new();
    for part in [scope, key] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    format!("{:x}", hash.finalize())
}
/// 块摘要用于校验不可信缓存字节。
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// 仅接受本实现产生的十六进制名字。
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
/// 数据块平铺存放，不接受调用方文件路径。
fn block_name(key: &str) -> String {
    format!("{key}.block")
}
/// 清理仅识别当前缓存格式的文件名。
fn owned_file(name: &str) -> bool {
    matches!(name, MANIFEST | "block.tmp" | "manifest.tmp")
        || name.strip_suffix(".block").is_some_and(valid_hash)
}
/// 保留旧缓存清单格式的写入时间字段，不用于磁盘配额淘汰。
fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u64::MAX as u128) as u64
}
/// 清单本身有独立大小限制，避免块数量导致无界索引。
fn encode(manifest: &Manifest) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(manifest).map_err(io::Error::other)?;
    if bytes.len() > MANIFEST_LIMIT {
        return Err(io::Error::other("缓存清单超限"));
    }
    Ok(bytes)
}
/// 缓存锁不可用时立即降级，保持界面可响应。
fn busy() -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, "缓存正在被使用")
}

#[cfg(test)]
mod tests;

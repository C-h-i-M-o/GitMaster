use super::*;

/// 新块持久化时不淘汰旧块，重新打开后仍可读取。
#[test]
fn cache_retains_disk_entries_after_reopen() {
    let root = tempfile::tempdir().unwrap();
    let cache = ContentCache::with_memory_limit(root.path(), 0).unwrap();
    for key in ["a", "b", "c"] {
        assert!(cache.put("repo", key, &[1; 1800]));
    }
    assert!(cache.usage().unwrap().disk_bytes > 5000);
    let reopened = ContentCache::with_memory_limit(root.path(), 0).unwrap();
    for key in ["a", "b", "c"] {
        assert_eq!(reopened.get("repo", key), Some(vec![1; 1800]));
    }
    assert_eq!(reopened.usage().unwrap().content_bytes, 5400);
}

/// 仓库身份隔离、损坏降级、清除只作用于缓存目录。
#[test]
fn cache_scopes_corruption_and_clear_are_bounded() {
    let root = tempfile::tempdir().unwrap();
    let cache = ContentCache::with_memory_limit(root.path(), 0).unwrap();
    assert!(cache.put("repo-a", "object", b"content"));
    assert!(cache.get("repo-b", "object").is_none());
    std::fs::write(
        root.path()
            .join(block_name(&key_digest("repo-a", "object"))),
        b"damaged",
    )
    .unwrap();
    assert!(cache.get("repo-a", "object").is_none());
    assert!(!root
        .path()
        .join(block_name(&key_digest("repo-a", "object")))
        .exists());
    assert!(cache.put("repo-a", "another", b"new"));
    cache.clear().unwrap();
    assert!(cache.get("repo-a", "another").is_none());
    assert!(cache.usage().unwrap().disk_bytes <= 1);
}

/// 锁竞争直接未命中，内存淘汰同样遵守配额，不等待另一进程。
#[test]
fn cache_lock_contention_falls_back_and_memory_stays_bounded() {
    let root = tempfile::tempdir().unwrap();
    let mut memory = Memory {
        limit: 8,
        ..Memory::default()
    };
    memory.insert("a".into(), b"12345678");
    memory.insert("b".into(), b"abcdefgh");
    assert!(!memory.entries.contains_key("a"));
    assert_eq!(memory.bytes, 8);
    let cache = ContentCache::with_memory_limit(root.path(), 8).unwrap();
    let lock = cache.disk.lock().unwrap();
    let other = ContentCache::with_memory_limit(root.path(), 0).unwrap();
    assert!(!other.put("repo", "locked", b"blocked"));
    assert!(other.get("repo", "locked").is_none());
    drop(lock);
    assert!(other.put("repo", "available", b"ok"));
}

/// 磁盘总量超过旧配额后，仍然保存新块且不淘汰既有块。
#[test]
fn cache_has_no_global_disk_quota() {
    let root = tempfile::tempdir().unwrap();
    let unrelated = std::fs::File::create(root.path().join("unrelated.dat")).unwrap();
    unrelated.set_len(1_000_000_001).unwrap();
    let cache = ContentCache::new(root.path()).unwrap();
    cache.put("repo", "new", b"persisted");
    drop(cache);
    let reopened = ContentCache::new(root.path()).unwrap();
    assert_eq!(reopened.get("repo", "new"), Some(b"persisted".to_vec()));
    reopened.clear().unwrap();
    assert!(root.path().join("unrelated.dat").exists());
}

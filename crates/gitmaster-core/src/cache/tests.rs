use super::*;

/// 小配额覆盖真实文件占用、最近使用淘汰与重新打开后的读取。
#[test]
fn cache_quota_includes_manifest_and_evicts_least_recently_used() {
    let root = tempfile::tempdir().unwrap();
    let cache = ContentCache::with_limits(root.path(), 5_000, 0).unwrap();
    assert!(cache.put("repo", "a", &[1; 1_800]));
    assert!(cache.put("repo", "b", &[2; 1_800]));
    assert_eq!(cache.get("repo", "a"), Some(vec![1; 1_800]));
    assert!(cache.put("repo", "c", &[3; 1_800]));
    assert!(cache.usage().unwrap().disk_bytes <= 5_000);
    assert!(cache.get("repo", "b").is_none());
    assert_eq!(cache.get("repo", "a"), Some(vec![1; 1_800]));
    let reopened = ContentCache::with_limits(root.path(), 5_000, 0).unwrap();
    assert_eq!(reopened.get("repo", "c"), Some(vec![3; 1_800]));
}

/// 仓库身份隔离、损坏降级、关闭和清除只作用于缓存目录。
#[test]
fn cache_scopes_corruption_disable_and_clear_are_bounded() {
    let root = tempfile::tempdir().unwrap();
    let cache = ContentCache::with_limits(root.path(), 20_000, 0).unwrap();
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
    cache.set_disk_enabled(false);
    assert!(!cache.put("repo-a", "another", b"new"));
    assert!(cache.get("repo-a", "another").is_none());
    cache.set_disk_enabled(true);
    assert!(cache.put("repo-a", "another", b"new"));
    cache.clear().unwrap();
    assert!(cache.get("repo-a", "another").is_none());
    assert!(cache.usage().unwrap().disk_bytes <= 1);
}

/// 锁竞争直接未命中，内存淘汰同样遵守配额，不等待另一进程。
#[test]
fn cache_lock_contention_falls_back_and_memory_stays_bounded() {
    let root = tempfile::tempdir().unwrap();
    let cache = ContentCache::with_limits(root.path(), 20_000, 8).unwrap();
    cache.set_disk_enabled(false);
    cache.put("repo", "a", b"12345678");
    cache.put("repo", "b", b"abcdefgh");
    assert!(cache.get("repo", "a").is_none());
    assert_eq!(cache.get("repo", "b"), Some(b"abcdefgh".to_vec()));
    assert_eq!(cache.usage().unwrap().memory_bytes, 8);
    cache.set_disk_enabled(true);
    let lock = cache.disk.lock().unwrap();
    let other = ContentCache::with_limits(root.path(), 20_000, 0).unwrap();
    assert!(!other.put("repo", "locked", b"blocked"));
    assert!(other.get("repo", "locked").is_none());
    drop(lock);
    assert!(other.put("repo", "available", b"ok"));
}

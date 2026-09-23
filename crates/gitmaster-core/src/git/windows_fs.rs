use super::OperationError;
use cap_std::fs::{Dir, Metadata, OpenOptions, OpenOptionsExt};
use std::path::{Component, Path};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
};

/// 固定版本的 cap-primitives 保留按句柄查询的卷号与文件 ID，不能用缺失值相等推断身份。
pub(super) fn identity(metadata: &Metadata) -> Option<(u32, u64)> {
    use cap_primitives::fs::_WindowsByHandle;
    Some((
        _WindowsByHandle::volume_serial_number(metadata)?,
        _WindowsByHandle::file_index(metadata)?,
    ))
}
/// 比较真实文件系统身份，任何一侧缺少身份都拒绝授权后续替换或删除。
pub(super) fn same(left: &Metadata, right: &Metadata) -> bool {
    matches!((identity(left),identity(right)),(Some(a),Some(b)) if a==b)
}
/// 统一拒绝包括 junction 在内的 reparse point，不仅判断普通符号链接。
pub(super) fn is_reparse(metadata: &Metadata) -> bool {
    use cap_std::fs::MetadataExt;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
/// 将最终对象作为自身打开，避免先查询路径再打开时跟随新链接。
pub(super) fn nofollow(options: &mut OpenOptions) {
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    // 此入口正是 cap-fs-ext 的适配点；依赖固定为 4.0.3，不额外下载包装 crate。
    options._cap_fs_ext_follow(cap_primitives::fs::FollowSymlinks::No);
}
/// 逐个单组件目录打开；成熟能力 API 处理句柄相对解析与链接边界。
pub(super) fn open_directory(parent: &Dir, name: &Path) -> Result<Dir, OperationError> {
    let mut parts = name.components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    let base = parent
        .try_clone()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?
        .into_std_file();
    let file = cap_primitives::fs::open_dir_nofollow(&base, name)
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let result = Dir::from_std_file(file);
    let metadata = result
        .dir_metadata()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    if !metadata.is_dir() || is_reparse(&metadata) || identity(&metadata).is_none() {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 同内容的新文件不能冒充原锁；已打开句柄仍代表被改名后的原对象。
    #[test]
    fn file_identity_distinguishes_replaced_lock() {
        let temporary = tempfile::tempdir().unwrap();
        let dir = Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority()).unwrap();
        dir.write("index.lock", b"index").unwrap();
        let original = dir.open("index.lock").unwrap();
        dir.rename("index.lock", &dir, "old.lock").unwrap();
        dir.write("index.lock", b"index").unwrap();
        let captured = original.metadata().unwrap();
        assert!(identity(&captured).is_some());
        assert!(same(&captured, &dir.symlink_metadata("old.lock").unwrap()));
        assert!(!same(
            &captured,
            &dir.symlink_metadata("index.lock").unwrap()
        ));
    }

    /// 能力目录只接受单一子组件，父级、绝对路径和多组件不能绕过边界。
    #[test]
    fn directory_handles_preserve_identity_and_component_boundary() {
        let temporary = tempfile::tempdir().unwrap();
        let dir = Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority()).unwrap();
        dir.create_dir("中文 目录").unwrap();
        let child = open_directory(&dir, Path::new("中文 目录")).unwrap();
        let again = open_directory(&dir, Path::new("中文 目录")).unwrap();
        assert!(same(
            &child.dir_metadata().unwrap(),
            &again.dir_metadata().unwrap()
        ));
        for path in ["..", ".", "中文 目录/child", "C:\\Windows"] {
            assert!(open_directory(&dir, Path::new(path)).is_err());
        }
        // cap-primitives 的目录能力句柄有意禁止 FILE_SHARE_DELETE，持有期间不能重命名。
        let captured = child.dir_metadata().unwrap();
        assert_eq!(
            dir.rename("中文 目录", &dir, "old")
                .unwrap_err()
                .raw_os_error(),
            Some(32)
        );
        drop(child);
        drop(again);
        dir.rename("中文 目录", &dir, "old").unwrap();
        dir.create_dir("中文 目录").unwrap();
        let replacement = open_directory(&dir, Path::new("中文 目录")).unwrap();
        assert!(!same(&captured, &replacement.dir_metadata().unwrap()));
    }
}

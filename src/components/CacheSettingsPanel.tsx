import type { useAppSettings } from "../hooks/useAppSettings";
import type { useSettingsForm } from "../hooks/useSettingsForm";
import { cacheSize, useContentCache } from "../hooks/useContentCache";
import { SettingsFieldError } from "./SettingsFieldError";

/** 展示全局缓存用量与用户控制项，实际操作交给后端固定目录接口。 */
export function CacheSettingsPanel({
  settings,
  form,
}: {
  settings: ReturnType<typeof useAppSettings>;
  form: ReturnType<typeof useSettingsForm>;
}) {
  const cache = useContentCache(settings.editable, settings.saved?.revision);
  return (
    <section>
      <h3>缓存</h3>
      <label className="checkbox-label">
        <input
          type="checkbox"
          data-settings-field="diskCacheEnabled"
          checked={settings.draft.diskCacheEnabled}
          onChange={form.diskCacheEnabled}
        />
        启用磁盘缓存
      </label>
      <SettingsFieldError error={settings.error} field="diskCacheEnabled" />
      <p>在多个项目间共享最多 1 GB 磁盘空间，用于加快已提交内容的再次读取。</p>
      <p className="muted small">
        关闭后停止磁盘读写，已有缓存保留；内存缓存仍可使用。清除不会删除项目文件、未保存草稿或应用设置。
      </p>
      {cache.usage && (
        <>
          <p>磁盘用量：{cacheSize(cache.usage.diskBytes)}</p>
          <p>磁盘上限：{cacheSize(cache.usage.limitBytes)}</p>
          <p>内存用量：{cacheSize(cache.usage.memoryBytes)}</p>
          <p>磁盘缓存当前{cache.usage.enabled ? "开启" : "关闭"}</p>
        </>
      )}
      <div className="button-row">
        <button
          className="secondary"
          disabled={cache.busy}
          onClick={cache.reload}
        >
          刷新用量
        </button>
        <button
          className="secondary"
          disabled={cache.busy}
          onClick={cache.clear}
        >
          清除缓存
        </button>
      </div>
      {cache.busy && <p role="status">正在处理缓存…</p>}
      {cache.error && <p role="alert">{cache.error}</p>}
    </section>
  );
}

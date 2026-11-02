import {
  cacheSize,
  cacheContentWidth,
  useContentCache,
} from "../hooks/useContentCache";

/** 展示应用缓存目录实际占用，仅保留受控清除操作。 */
export function CacheSettingsPanel({ enabled }: { enabled: boolean }) {
  const cache = useContentCache(enabled);
  return (
    <section className="cache-settings">
      <h3>缓存</h3>
      <div className="cache-usage-header">
        <div>
          <span className="muted">磁盘占用</span>
          <strong className="cache-total">
            {cache.usage ? cacheSize(cache.usage.diskBytes) : "—"}
          </strong>
        </div>
        <button
          className="secondary"
          disabled={!enabled || cache.busy}
          onClick={cache.clear}
        >
          清除缓存
        </button>
      </div>
      {cache.usage && (
        <figure
          className="cache-chart"
          aria-label={`缓存占用 ${cache.usage.diskBytes.toLocaleString("zh-CN")} 字节，其中内容 ${cache.usage.contentBytes.toLocaleString("zh-CN")} 字节`}
        >
          <div
            className={`cache-bar${cache.usage.diskBytes === 0 ? " empty" : ""}`}
            aria-hidden="true"
          >
            <span style={cacheContentWidth(cache.usage)} />
          </div>
          <figcaption>
            <span>
              <i className="cache-content-dot" />
              内容 {cacheSize(cache.usage.contentBytes)}
            </span>
            <span>
              <i className="cache-index-dot" />
              索引与其他{" "}
              {cacheSize(cache.usage.diskBytes - cache.usage.contentBytes)}
            </span>
          </figcaption>
        </figure>
      )}
      {cache.busy && (
        <p className="muted small" role="status">
          正在更新缓存用量…
        </p>
      )}
      {cache.error && <p role="alert">{cache.error}</p>}
    </section>
  );
}

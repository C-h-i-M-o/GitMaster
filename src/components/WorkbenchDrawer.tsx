import { EditorDialogs } from "./EditorDialogs";
import { ProjectFilesPanel } from "./ProjectFilesPanel";
import { useConflictScroll } from "../hooks/useConflictScroll";
import type { Workbench } from "../hooks/useWorkbench";
import { VirtualChanges } from "./VirtualChanges";
import { Icon } from "./Icon";
import { ContentPreview } from "./ContentPreview";
import { describeGitError } from "../ui/gitPresentation";
/** 文件勾选与差异查看分开，提交范围始终是整个已暂存索引。 */
function Changes({ w }: { w: Workbench }) {
  return (
    <>
      <div className="changes-content">
        <div className="drawer-scroll changes-list-region">
          <div className="section-heading">
            <h3>选择这次要保存的修改</h3>
            <span>{w.repo.repository?.changes.length ?? 0} 个文件</span>
          </div>
          <VirtualChanges
            groups={w.groups}
            selected={w.selected}
            blocked={w.blocked}
            toggle={w.toggleChange}
            inspect={w.inspectChange}
          />
          {w.groups.conflicted.length > 0 && (
            <button
              className="warning-card"
              onClick={w.openDrawer("conflicts")}
            >
              {w.groups.conflicted.length} 个文件存在冲突，打开冲突处理
            </button>
          )}
          {w.repo.error && <p role="alert">{describeGitError(w.repo.error)}</p>}
        </div>
        <div className="drawer-bottom">
          {w.writeReason("commit") && (
            <p className="muted small">{w.writeReason("commit")}</p>
          )}
          <label>
            提交说明
            <textarea
              value={w.fields.message}
              onChange={w.field("message")}
              placeholder="这次修改解决了什么问题？"
              rows={3}
              disabled={w.operations.busy}
            />
          </label>
        </div>
      </div>
      <div className="changes-footer">
        <p className="muted small">
          提交会包含全部 {w.groups.staged.length} 个已暂存文件。
        </p>
        <p className="muted small" id="selection-reason">
          {w.selection.reason}
        </p>
        <div className="changes-actions">
          <button
            className="secondary"
            disabled={
              w.selection.kind === "none" ||
              w.selection.kind === "mixed" ||
              !w.canWrite(w.selection.kind)
            }
            aria-describedby="selection-reason"
            onClick={w.prepareSelected}
          >
            {w.selection.label}
          </button>
          <button
            className="primary"
            disabled={!w.canWrite("commit") || !w.fields.message.trim()}
            title={w.writeReason("commit")}
            onClick={w.prepareCommit}
          >
            提交
          </button>
        </div>
      </div>
    </>
  );
}
/** 展示完整提交身份与直接父比较选择，不隐藏合并父节点。 */
function CommitDetails({ w }: { w: Workbench }) {
  const detail = w.history.detail;
  return (
    <div className="drawer-scroll">
      {w.history.detailLoading ? (
        <p role="status">正在读取提交…</p>
      ) : detail ? (
        <>
          <h2>{detail.subject}</h2>
          <dl className="metadata">
            <dt>完整提交 ID</dt>
            <dd className="mono">{detail.oid}</dd>
            <dt>作者</dt>
            <dd>
              {detail.authorName} &lt;{detail.authorEmail}&gt;
            </dd>
            <dt>作者时间</dt>
            <dd>{detail.authoredAt}</dd>
            <dt>提交者</dt>
            <dd>
              {detail.committerName} &lt;{detail.committerEmail}&gt;
            </dd>
            <dt>提交时间</dt>
            <dd>{detail.committedAt}</dd>
          </dl>
          <pre className="commit-body">{detail.body}</pre>
          {detail.truncated && <p role="status">提交说明已截断。</p>}
          {detail.parentOids.length ? (
            <label>
              比较父提交
              <select
                value={w.history.files?.parentOid ?? detail.parentOids[0]}
                onChange={w.changeParent}
              >
                {detail.parentOids.map((oid, index) => (
                  <option key={oid} value={oid}>
                    父 {index + 1} · {oid}
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <p className="muted">根提交 · 与空树比较</p>
          )}
          <h3>变更文件</h3>
          {w.history.filesLoading ? (
            <p role="status">正在读取文件列表…</p>
          ) : (
            w.history.files?.files.map((file) => (
              <button
                key={file.fileId}
                className="file-row file-button"
                onClick={w.inspectCommitFile(file.fileId)}
              >
                <span className="file-name">
                  {file.originalPath ? `${file.originalPath} → ` : ""}
                  {file.path}
                </span>
                <span className="diff-count">
                  +{file.additions ?? "—"} −{file.deletions ?? "—"}
                </span>
              </button>
            ))
          )}
          <ContentPreview
            value={w.history.diff}
            loading={w.history.diffLoading}
          />
        </>
      ) : (
        <p>在提交图中选择一个节点。</p>
      )}
      {w.history.error && (
        <p role="alert">{describeGitError(w.history.error)}</p>
      )}
    </div>
  );
}
/** 三栏显示原始两侧与可编辑结果，采用一侧不会自动写入。 */
function Conflicts({ w }: { w: Workbench }) {
  const state = w.conflicts;
  const scroll = useConflictScroll(
    state.document?.local ?? null,
    state.document?.incoming ?? null,
    state.draft,
  );
  const selectedFile = state.conflicts?.files.find(
    (file) => file.conflictId === state.document?.conflictId,
  );
  return (
    <>
      <div className="drawer-scroll">
        <div className="section-heading">
          <p className="muted">逐个保存并暂存后，再完成合并。</p>
          <button
            className="secondary"
            disabled={w.operations.busy || state.loading}
            onClick={w.refreshConflicts}
          >
            重新读取
          </button>
        </div>
        <div className="conflict-files">
          {state.conflicts?.files.map((file) => (
            <button
              key={file.conflictId}
              className={`file-row file-button ${file.conflictId === state.document?.conflictId ? "active" : ""}`}
              disabled={
                file.editorSupport.status !== "supported" || w.operations.busy
              }
              onClick={w.selectConflict(file.conflictId)}
            >
              <span>{file.path}</span>
              {file.editorSupport.status === "unsupported" && (
                <small>请使用外部工具处理：{file.editorSupport.reason}</small>
              )}
            </button>
          ))}
        </div>
        {state.error && <p role="alert">{describeGitError(state.error)}</p>}
        {state.stale && (
          <p className="warning-card" role="alert">
            仓库或文件已变化。保留当前草稿，重新读取前请确认是否放弃。
          </p>
        )}
        {state.documentLoading ? (
          <p role="status">正在读取冲突文件…</p>
        ) : state.document ? (
          <>
            <p className="muted small">
              {state.document.encoding} · {state.document.lineEnding} ·{" "}
              {state.dirty ? "草稿尚未保存" : "内容已读取"}
            </p>
            <details className="conflict-oids">
              <summary>查看三个版本的对象 ID</summary>
              <dl className="metadata">
                <dt>共同基线</dt>
                <dd>{selectedFile?.stageOids.base ?? "无"}</dd>
                <dt>本地</dt>
                <dd>{selectedFile?.stageOids.local ?? "无"}</dd>
                <dt>传入</dt>
                <dd>{selectedFile?.stageOids.incoming ?? "无"}</dd>
              </dl>
            </details>
            <div className="merge-editors">
              <section>
                <h3>本地版本</h3>
                <button
                  className="secondary"
                  disabled={state.stale || w.operations.busy}
                  onClick={w.useConflictSide("local")}
                >
                  采用本地全文
                </button>
                <pre
                  tabIndex={0}
                  ref={scroll.localRef}
                  onScroll={scroll.onScroll}
                >
                  {scroll.localLines.join("\n")}
                </pre>
              </section>
              <section>
                <h3>合并结果</h3>
                <span className="muted small">可编辑，保存时写入并暂存</span>
                <div
                  className="merge-result-scroll"
                  ref={scroll.resultRef}
                  onScroll={scroll.onScroll}
                >
                  <textarea
                    aria-label="合并结果"
                    style={scroll.resultStyle}
                    wrap="off"
                    spellCheck={false}
                    value={state.draft}
                    onChange={w.editConflict}
                    disabled={state.stale || w.operations.busy}
                  />
                </div>
              </section>
              <section>
                <h3>传入版本</h3>
                <button
                  className="secondary"
                  disabled={state.stale || w.operations.busy}
                  onClick={w.useConflictSide("incoming")}
                >
                  采用传入全文
                </button>
                <pre
                  tabIndex={0}
                  ref={scroll.incomingRef}
                  onScroll={scroll.onScroll}
                >
                  {scroll.incomingLines.join("\n")}
                </pre>
              </section>
            </div>
            <button
              className="primary"
              disabled={state.stale || w.operations.busy || state.loading}
              onClick={w.prepareSaveConflict}
            >
              预览保存并暂存
            </button>
          </>
        ) : (
          <p className="muted">
            选择一个支持的文本冲突；外部工具处理后请重新读取。
          </p>
        )}
      </div>
      <div className="drawer-bottom">
        <label>
          合并提交说明
          <textarea
            value={w.fields.mergeMessage}
            onChange={w.field("mergeMessage")}
            rows={2}
            placeholder="说明这次合并的目的"
          />
        </label>
        <p className="muted small">
          后端会核对全部冲突已解决，并保留两个合并父提交。
        </p>
        <button
          className="primary"
          disabled={!w.canWrite("finishMerge") || !w.fields.mergeMessage.trim()}
          title={w.writeReason("finishMerge")}
          onClick={w.prepareFinishMerge}
        >
          预览完成合并
        </button>
      </div>
    </>
  );
}
/** 根据工作台导航显示右侧业务抽屉。 */
export function WorkbenchDrawer({ workbench: w }: { workbench: Workbench }) {
  return (
    <>
      <aside
        hidden={!w.drawer}
        className={`workbench-drawer ${w.drawer === "conflicts" ? "merge-drawer" : w.drawer === "files" ? "files-drawer" : ""}`}
        aria-label={
          {
            changes: "本地修改",
            files: "项目文件",
            detail: "提交详情",
            conflicts: "冲突处理",
          }[w.drawer ?? "files"]
        }
      >
        <header className="drawer-header">
          <h2>
            {
              {
                changes: "本地修改",
                files: "项目文件",
                detail: "提交详情",
                conflicts: "冲突处理",
              }[w.drawer ?? "files"]
            }
          </h2>
          <button
            className="icon-button"
            aria-label="关闭侧栏"
            onClick={w.closeDrawer}
          >
            <Icon name="close" />
          </button>
        </header>
        {w.drawer === "changes" ? (
          <Changes w={w} />
        ) : w.drawer === "detail" ? (
          <CommitDetails w={w} />
        ) : w.drawer === "conflicts" ? (
          <Conflicts w={w} />
        ) : null}
        <div className="project-files-host" hidden={w.drawer !== "files"}>
          <ProjectFilesPanel key={w.repo.repository?.repositoryId} w={w} />
        </div>
      </aside>
      <EditorDialogs w={w} />
    </>
  );
}

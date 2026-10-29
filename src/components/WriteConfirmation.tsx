import type { ReactNode } from "react";
import type { Workbench } from "../hooks/useWorkbench";
import { useNativeDialog } from "../hooks/useNativeDialog";
import { useDestructiveConfirmation } from "../hooks/useDestructiveConfirmation";
import {
  describeOperationKind,
  formatOperationExpiry,
} from "../ui/operationPresentation";

interface WriteConfirmationProps {
  workbench: Workbench;
}

/** 展示后端签发的写入或克隆预览，并把确认动作交还工作台。 */
export function WriteConfirmation({
  workbench,
}: WriteConfirmationProps): ReactNode {
  const preview = workbench.operations.immediate
    ? null
    : workbench.operations.preview;
  const dialog = useNativeDialog(
    preview !== null,
    workbench.operations.discardPreview,
  );
  const isClean =
    preview?.type === "write" && preview.value.kind === "cleanUntracked";
  const isAbort =
    preview?.type === "write" && preview.value.kind === "abortRevert";
  const requiresAcknowledgement = isClean || isAbort;
  const destructive = useDestructiveConfirmation(
    preview?.value.planId,
    requiresAcknowledgement,
    workbench.operations.confirm,
  );
  if (!preview)
    return (
      <dialog
        ref={dialog.dialogRef}
        className="gm-dialog"
        hidden
        aria-hidden="true"
      />
    );
  const busy = workbench.operations.activity !== "idle";
  const close = busy ? undefined : workbench.operations.discardPreview;
  return (
    <dialog
      ref={dialog.dialogRef}
      className="gm-dialog"
      aria-labelledby="write-confirmation-title"
      onCancel={dialog.onCancel}
    >
      <div className="dialog-body">
        {preview.type === "clone" ? (
          <>
            <h2 id="write-confirmation-title">确认克隆仓库</h2>
            <dl>
              <dt>目标路径</dt>
              <dd>{preview.value.targetPath}</dd>
              <dt>远端地址</dt>
              <dd>{preview.value.displayUrl}</dd>
            </dl>
            <p>
              预览有效期至：{formatOperationExpiry(preview.value.expiresAt)}
            </p>
          </>
        ) : (
          <>
            <h2 id="write-confirmation-title">{`确认${describeOperationKind(preview.value.kind)}`}</h2>
            <WriteDetails value={preview.value} />
            <p>
              预览有效期至：{formatOperationExpiry(preview.value.expiresAt)}
            </p>
          </>
        )}
        <div className="dialog-actions">
          {requiresAcknowledgement && (
            <label className="clean-confirmation">
              <input
                type="checkbox"
                checked={destructive.accepted}
                onChange={destructive.change}
                disabled={busy}
              />
              {isClean
                ? "我已核对清单，确认删除；这些文件无法通过 Git 恢复。"
                : "我已核对影响范围，确认丢弃当前冲突编辑和暂存区修改。"}
            </label>
          )}
          <button
            className="secondary"
            type="button"
            onClick={close}
            disabled={busy}
          >
            取消
          </button>
          <button
            className="primary"
            type="button"
            onClick={destructive.confirm}
            disabled={
              busy || (requiresAcknowledgement && !destructive.accepted)
            }
          >
            确认执行
          </button>
        </div>
      </div>
    </dialog>
  );
}

/** 展示写入目标、提交信息、父提交和后端警告。 */
function WriteDetails({
  value,
}: {
  value: Extract<
    Workbench["operations"]["preview"],
    { type: "write" }
  >["value"];
}): ReactNode {
  const recovery =
    value.kind === "restoreFiles" ||
    value.kind === "cleanUntracked" ||
    value.kind === "revert" ||
    value.kind === "abortRevert";
  return (
    <dl>
      <dt>当前 HEAD</dt>
      <dd>
        {value.head.kind === "branch"
          ? `${value.head.name}（${value.head.oid}）`
          : value.head.kind === "detached"
            ? `游离 HEAD（${value.head.oid}）`
            : `未出生分支（${value.head.name}）`}
      </dd>
      {!recovery && (
        <>
          <dt>父提交</dt>
          <dd>
            {value.parentOids.length > 0 ? (
              <ul>
                {value.parentOids.map((oid) => (
                  <li key={oid}>{oid}</li>
                ))}
              </ul>
            ) : (
              "根提交（无父提交）"
            )}
          </dd>
        </>
      )}
      <dt>
        {value.kind === "revert"
          ? "原提交涉及路径"
          : value.kind === "abortRevert"
            ? "当前已跟踪变化"
            : "路径"}
      </dt>
      <dd>
        {value.paths.length > 0 ? (
          <ul>
            {value.paths.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        ) : (
          "无路径"
        )}
      </dd>
      {!recovery && (
        <>
          <dt>作者</dt>
          <dd>{value.author ?? "未提供"}</dd>
          <dt>说明</dt>
          <dd>{value.message ?? "未提供"}</dd>
        </>
      )}
      <dt>目标</dt>
      <dd>{value.target ? <Target target={value.target} /> : "当前索引"}</dd>
      {value.warnings.length > 0 && (
        <>
          <dt>警告</dt>
          <dd>
            <ul>
              {value.warnings.map((warning, index) => (
                <li key={`${warning}-${index}`}>{warning}</li>
              ))}
            </ul>
          </dd>
        </>
      )}
    </dl>
  );
}

/** 以语义文本展示后端签发的写入目标及待提交项。 */
function Target({
  target,
}: {
  target: NonNullable<
    Extract<
      Workbench["operations"]["preview"],
      { type: "write" }
    >["value"]["target"]
  >;
}): ReactNode {
  if (target.kind === "revert") return <span>被撤销提交：{target.oid}</span>;
  if (target.kind === "clean")
    return (
      <ul>
        {target.files.map((file) => (
          <li key={file.path}>
            {file.path} · {file.bytes} 字节
            <small className="clean-file-hash">SHA-256：{file.sha256}</small>
          </li>
        ))}
      </ul>
    );
  if (target.kind === "restore")
    return (
      <div>
        <p>
          {target.source.kind === "index"
            ? "暂存区"
            : target.source.kind === "head"
              ? "当前提交"
              : "历史提交"}{" "}
          {target.sourceOid ?? ""}
        </p>
        {target.files.map((file) => (
          <details key={file.path}>
            <summary>
              {file.path} ·{" "}
              {file.before
                ? `${file.before.bytes} → ${file.after.bytes} 字节`
                : `恢复缺失文件，${file.after.bytes} 字节`}
            </summary>
            <p className="small">
              原内容 SHA-256：{file.before?.sha256 ?? "文件不存在"}
            </p>
            <p className="small">目标 SHA-256：{file.after.sha256}</p>
            {file.after.text !== null &&
            (file.before === null || file.before.text !== null) ? (
              <div className="restore-comparison">
                <section>
                  <h4>当前工作区</h4>
                  <pre>{file.before?.text ?? "（文件不存在）"}</pre>
                </section>
                <section>
                  <h4>恢复后</h4>
                  <pre>{file.after.text}</pre>
                </section>
              </div>
            ) : (
              <p>二进制、大文件或批量选择仅展示完整内容摘要。</p>
            )}
          </details>
        ))}
      </div>
    );
  if (target.kind === "localBranch")
    return (
      <span>
        {target.name}（目标 OID：{target.oid ?? "无提交"}）
      </span>
    );
  if (target.kind === "remote")
    return (
      <span>
        <span>
          {target.displayUrl} {target.refName}（source OID：
          {target.sourceOid ?? "无"}，target OID：{target.oid ?? "无"}）
        </span>
        <ul>
          {target.pendingCommits.length > 0 ? (
            target.pendingCommits.map((commit) => (
              <li key={commit.oid}>
                {commit.oid}：{commit.subject}（{commit.authorName}）
              </li>
            ))
          ) : (
            <li>无待提交</li>
          )}
        </ul>
      </span>
    );
  return (
    <span>
      合并 {target.remoteBranchId}（target OID：{target.oid}）
    </span>
  );
}

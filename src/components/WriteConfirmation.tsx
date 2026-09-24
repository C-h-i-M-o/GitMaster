import type { ReactNode } from "react";
import type { Workbench } from "../hooks/useWorkbench";
import { useNativeDialog } from "../hooks/useNativeDialog";
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
            onClick={workbench.operations.confirm}
            disabled={busy}
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
      <dt>路径</dt>
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
      <dt>作者</dt>
      <dd>{value.author ?? "未提供"}</dd>
      <dt>说明</dt>
      <dd>{value.message ?? "未提供"}</dd>
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

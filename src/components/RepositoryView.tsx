import type { RepositoryViewState } from "../hooks/useRepository";
import {
  groupChanges,
  describeGitError,
  describeHead,
  describeOperation,
} from "../ui/gitPresentation";
import type { DiffSide } from "../types/git";
import { useRepositoryView } from "./useRepositoryView";
import { FileChangeList } from "./FileChangeList";
import { DiffView } from "./DiffView";
interface Props extends RepositoryViewState {
  onOpen: () => void;
  onRefresh: () => void;
  onSelect: (id: string, side: DiffSide) => void;
  disabled: boolean;
  selected: { changeId: string; side: DiffSide } | null;
}
/** 展示仓库快照并组合文件列表和差异面板。 */
export function RepositoryView({
  repository,
  loading,
  stale,
  error,
  diff,
  diffLoading,
  onOpen,
  onRefresh,
  onSelect,
  selected = null,
  disabled,
}: Props) {
  const view = useRepositoryView(onSelect, repository?.snapshotId ?? null);
  if (!repository)
    return (
      <section className="repository-panel">
        <h2>打开一个仓库</h2>
        <p>选择已有工作区，gitMaster 只读取状态。</p>
        {loading && <p role="status">正在打开仓库…</p>}
        <button type="button" disabled={disabled || loading} onClick={onOpen}>
          选择仓库目录
        </button>
        {error && <p role="alert">{describeGitError(error)}</p>}
      </section>
    );
  const groups = groupChanges(repository.changes);
  const selectedChange = selected
    ? repository.changes.find((change) => change.changeId === selected.changeId)
    : undefined;
  return (
    <section className="repository-panel">
      <div className="repo-header">
        <div>
          <h2>{repository.rootPath}</h2>
          <p>
            {describeHead(
              repository.head.kind,
              repository.head.kind === "branch" ||
                repository.head.kind === "unborn"
                ? repository.head.name
                : undefined,
            )}
          </p>
          {repository.operations.length > 0 && (
            <p>{repository.operations.map(describeOperation).join("、")}</p>
          )}
          {repository.changes.length === 0 && (
            <p>工作区干净，没有待处理修改。</p>
          )}
        </div>
        <div className="panel-actions">
          <button type="button" disabled={disabled || loading} onClick={onOpen}>
            切换仓库
          </button>
          <button
            type="button"
            disabled={disabled || loading}
            onClick={onRefresh}
          >
            刷新
          </button>
        </div>
      </div>
      {stale && (
        <p className="stale-note" role="status">
          显示的是上一次成功读取的内容，已过期。
        </p>
      )}
      {loading && <p role="status">正在刷新…</p>}
      <div className="change-groups">
        <FileChangeList
          changes={groups.staged}
          side="staged"
          {...view}
          onSelect={view.select}
          onMore={view.more}
          disabled={loading || disabled}
        />
        <FileChangeList
          changes={groups.unstaged}
          side="unstaged"
          {...view}
          onSelect={view.select}
          onMore={view.more}
          disabled={loading || disabled}
        />
        <FileChangeList
          changes={groups.untracked}
          side="untracked"
          {...view}
          onSelect={view.select}
          onMore={view.more}
          disabled={loading || disabled}
        />
        <FileChangeList
          changes={groups.conflicted}
          side="unstaged"
          title="冲突"
          {...view}
          onSelect={view.select}
          onMore={view.more}
          disabled={loading || disabled}
        />
      </div>
      <DiffView
        scope={{
          repositoryId: repository.repositoryId,
          snapshotId: repository.snapshotId,
        }}
        diff={diff}
        loading={diffLoading}
        selected={selected}
        change={selectedChange}
      />
      {error && <p role="alert">{describeGitError(error)}</p>}
    </section>
  );
}

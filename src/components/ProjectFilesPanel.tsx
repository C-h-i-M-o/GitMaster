import type { Workbench } from "../hooks/useWorkbench";
import { useFileTree } from "../hooks/useFileTree";
import type { FileTreeNode } from "../ui/fileTree";
import { ContentPreview } from "./ContentPreview";
import { useProjectFolder } from "../hooks/useProjectFolder";
import { describeGitError } from "../ui/gitPresentation";

/** 递归目录列表采用原生按钮，支持键盘访问及展开状态朗读。 */
function FileNodes({
  nodes,
  tree,
  w,
}: {
  nodes: FileTreeNode[];
  tree: ReturnType<typeof useFileTree>;
  w: Workbench;
}) {
  return (
    <ul className="project-tree-list">
      {nodes.map((node) => (
        <li key={`${node.kind}:${node.path}`}>
          {node.kind === "directory" ? (
            <>
              <button
                className="project-tree-row"
                title={node.path}
                aria-expanded={tree.isOpen(node.path)}
                onClick={tree.toggle(node.path)}
              >
                <span aria-hidden="true">
                  {tree.isOpen(node.path) ? "▾" : "▸"}
                </span>
                <span>{node.name}</span>
              </button>
              {tree.isOpen(node.path) && (
                <FileNodes nodes={node.children} tree={tree} w={w} />
              )}
            </>
          ) : (
            <button
              className={`project-tree-row ${w.projectPath === node.path ? "selected" : ""}`}
              title={node.path}
              aria-current={w.projectPath === node.path ? "true" : undefined}
              onClick={w.inspectProjectFile(node.fileId)}
            >
              <span aria-hidden="true">◇</span>
              <span>{node.name}</span>
              {node.status === "untracked" && <small>新文件</small>}
              {node.status === "ignored" && <small>已忽略</small>}
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}

/** 项目正文置左、文件树置右，读取仍使用已有受限后端入口。 */
export function ProjectFilesPanel({ w }: { w: Workbench }) {
  const tree = useFileTree(w.projectFiles);
  const folder = useProjectFolder(
    w.repo.repository?.repositoryId,
    !w.preview,
    w.appSettings.saved?.settings.externalOpen.defaultAppId ?? "fileManager",
  );
  return (
    <div className="project-files-panel">
      <section className="project-file-content" aria-label="文件内容">
        <div className="button-row">
          <select
            aria-label="外部打开方式"
            value={folder.selected}
            onChange={folder.select}
            disabled={folder.busy || w.preview}
          >
            <option value="fileManager">文件管理器</option>
            <option value="vsCode" disabled={!folder.availability?.vsCode}>
              VS Code{folder.availability?.vsCode ? "" : "（未找到）"}
            </option>
            <option value="terminal" disabled={!folder.availability?.terminal}>
              系统终端{folder.availability?.terminal ? "" : "（未找到）"}
            </option>
          </select>
          <button
            className="secondary"
            disabled={
              w.preview ||
              folder.busy ||
              !w.repo.repository ||
              !folder.available
            }
            onClick={folder.open}
          >
            打开项目文件夹
          </button>
        </div>
        {!folder.available && (
          <p role="status">所选应用尚不可用，请选择其他打开方式。</p>
        )}
        {folder.error && <p role="alert">{describeGitError(folder.error)}</p>}
        <h3 className="preview-path">{w.projectPath || "选择项目文件"}</h3>
        <ContentPreview value={w.projectDiff} loading={w.projectLoading} />
      </section>
      <nav className="project-file-tree" aria-label="项目文件">
        <label>
          筛选文件
          <input
            type="search"
            value={tree.query}
            onChange={tree.filter}
            placeholder="按路径筛选…"
          />
        </label>
        <label className="sync-create-option">
          <input
            type="checkbox"
            checked={w.includeIgnored}
            onChange={w.changeIncludeIgnored}
            disabled={w.preview}
          />
          显示忽略文件
        </label>
        <p className="muted small">
          {w.includeIgnored
            ? "包含忽略文件；隐藏 Git 元数据"
            : "已跟踪与未忽略文件"}
        </p>
        <FileNodes nodes={tree.nodes} tree={tree} w={w} />
        {!tree.nodes.length && (
          <p className="muted">
            {w.projectLoading ? "正在读取文件…" : "没有匹配文件"}
          </p>
        )}
      </nav>
    </div>
  );
}

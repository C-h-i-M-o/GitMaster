import { Icon } from "./Icon";
import type { Workbench } from "../hooks/useWorkbench";
import { Suspense } from "react";
import { FileEditor } from "../ui/editorLoader";
import { defaultSettings } from "../ui/settingsDraft";
import { useFileTree } from "../hooks/useFileTree";
import { VirtualFileTree } from "./VirtualFileTree";
import { ContentPreview } from "./ContentPreview";
import { useProjectFolder } from "../hooks/useProjectFolder";
import { describeGitError } from "../ui/gitPresentation";
import { PagedReader } from "./PagedReader";
import { useProjectFilesView } from "../hooks/useProjectFilesView";
import { shortcutLabel } from "../ui/shortcuts";

/** 项目正文置左、文件树置右，读取仍使用已有受限后端入口。 */
export function ProjectFilesPanel({ w }: { w: Workbench }) {
  const tree = useFileTree(
    w.projectTree,
    w.loadProjectDirectory,
    w.searchProjectFiles,
  );
  const selected = w.editor.tabs.find(
    (tab) => tab.document.path === w.projectPath,
  );
  const readOnlyFile =
    !selected &&
    !w.editor.loading &&
    (w.editor.error?.code === "FILE_EDIT_TOO_LARGE" ||
      w.editor.error?.code === "FILE_EDIT_LINE_ENDING")
      ? w.projectFiles?.files.find((file) => file.path === w.projectPath)
      : undefined;
  const folder = useProjectFolder(
    w.repo.repository?.repositoryId,
    !w.preview,
    w.appSettings.saved?.settings.externalOpen.defaultAppId ?? "fileManager",
  );
  const view = useProjectFilesView(w.projectPath, w.editor.tabs, tree.reveal);
  return (
    <div
      className={`project-files-panel ${view.treeVisible ? "" : "tree-hidden"}`}
    >
      <header className="project-files-tabs" aria-label="已打开文档">
        {w.editor.tabs.map((tab) => (
          <div
            className={`project-files-tab ${tab.document.path === w.editor.activePath ? "active" : ""}`}
            key={tab.document.path}
            title={`${tab.document.path} · 保存 ${shortcutLabel("S")}`}
          >
            <button
              aria-pressed={tab.document.path === w.editor.activePath}
              onClick={w.selectEditor(tab.document.path)}
            >
              <Icon name="files" />
              {view.tabLabel(tab.document.path)}
              {tab.draft !== tab.baseline ? "*" : ""}
            </button>
            <button
              aria-label={`关闭 ${tab.document.path}`}
              onClick={w.closeEditor(tab.document.path)}
            >
              ×
            </button>
          </div>
        ))}
        <button
          className="project-files-close"
          aria-label="关闭项目文件面板"
          onClick={w.closeDrawer}
        >
          ×
        </button>
      </header>
      <div className="project-files-subbar">
        <nav aria-label="当前文件路径" className="project-files-breadcrumb">
          <button onClick={view.locate("")} title={w.repo.repository?.rootPath}>
            {w.repo.repository?.rootPath
              .split(/[\\/]/)
              .filter(Boolean)
              .at(-1) ?? "项目"}
          </button>
          {view.breadcrumbs.map((part) => (
            <span key={part.path}>
              {part.directory ? (
                <button onClick={view.locate(part.path)} title={part.path}>
                  {part.name}
                </button>
              ) : (
                <span title={part.path}>{part.name}</span>
              )}
            </span>
          ))}
        </nav>
        <button
          className="project-tree-toggle"
          onClick={view.toggleTree}
          title={view.treeVisible ? "隐藏右侧目录" : "显示右侧目录"}
          aria-label={view.treeVisible ? "隐藏右侧目录" : "显示右侧目录"}
          aria-expanded={view.treeVisible}
        >
          <Icon name="folder" />
        </button>
        <div className="project-files-open">
          <select
            aria-label="外部打开方式"
            value={folder.selected}
            onChange={folder.select}
            disabled={folder.busy || w.preview}
          >
            <option value="fileManager">文件管理器</option>
            <option value="vsCode" disabled={!folder.availability?.vsCode}>
              VS Code
            </option>
            <option value="terminal" disabled={!folder.availability?.terminal}>
              系统终端
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
            打开 ▾
          </button>
        </div>
      </div>
      <section className="project-file-content" aria-label="文件内容">
        {w.resourceError && (
          <p role="alert">{describeGitError(w.resourceError)}</p>
        )}
        {!folder.available && (
          <p role="status">所选应用尚不可用，请选择其他打开方式。</p>
        )}
        {folder.error && <p role="alert">{describeGitError(folder.error)}</p>}
        {w.editor.error && !readOnlyFile && (
          <p role="alert">
            {describeGitError(w.editor.error)}{" "}
            <button type="button" onClick={w.saveActiveEditor}>
              重试保存
            </button>{" "}
            <button type="button" onClick={w.reloadEditor}>
              重新读取
            </button>
          </p>
        )}
        <div className="editor-host" hidden={!selected}>
          {w.editor.tabs.length > 0 && (
            <Suspense fallback={<p role="status">正在加载编辑器…</p>}>
              <FileEditor
                repositoryId={w.editor.repositoryId ?? ""}
                tabs={w.editor.tabs}
                activePath={selected?.document.path ?? null}
                preferences={
                  w.appSettings.saved?.settings.editor ??
                  defaultSettings().editor
                }
                edit={w.editDocument}
                compositionStart={w.editorCompositionStart}
                compositionEnd={w.editorCompositionEnd}
                readOnly={
                  w.choosing ||
                  w.editorLeaving ||
                  ((w.operations.busy || w.repo.loading) &&
                    !w.editor.savingPath)
                }
                save={w.saveEditor}
              />
            </Suspense>
          )}
        </div>
        {readOnlyFile && w.projectFiles && (
          <PagedReader
            key={`${w.projectFiles.snapshotId}:${readOnlyFile.fileId}`}
            scope={w.projectFiles}
            fileId={readOnlyFile.fileId}
          />
        )}
        {!selected && !readOnlyFile && (
          <ContentPreview value={w.projectDiff} loading={w.projectLoading} />
        )}
      </section>
      <nav
        className="project-file-tree"
        aria-label="项目文件"
        hidden={!view.treeVisible}
      >
        <label>
          <span className="sr-only">筛选文件</span>
          <input
            type="search"
            maxLength={256}
            value={tree.query}
            onChange={tree.filter}
            placeholder="按路径筛选…"
          />
        </label>
        <VirtualFileTree
          tree={tree}
          selectedPath={w.projectPath}
          openFile={w.inspectProjectFile}
        />
        {tree.query.trim() && (
          <p className="muted small" role="status">
            {tree.searching
              ? "正在搜索文件…"
              : tree.searchIncomplete
                ? "搜索尚未完成，可继续搜索更多目录。"
                : `找到 ${tree.nodes.length} 个匹配文件`}
          </p>
        )}
        {tree.searchIncomplete && (
          <button disabled={tree.searching} onClick={tree.more("")}>
            继续搜索
          </button>
        )}
        {tree.error && <p role="alert">{tree.error}</p>}
        {!tree.nodes.length && (
          <p className="muted">
            {w.projectLoading || tree.loading("")
              ? "正在读取文件…"
              : "没有匹配文件"}
          </p>
        )}
      </nav>
    </div>
  );
}

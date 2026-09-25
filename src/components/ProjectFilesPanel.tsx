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
        <div className="editor-tabs" aria-label="已打开文档">
          {w.editor.tabs.map((tab) => (
            <div key={tab.document.path}>
              <button
                aria-pressed={tab.document.path === w.editor.activePath}
                onClick={w.selectEditor(tab.document.path)}
              >
                {tab.document.path}
                {tab.draft !== tab.baseline ? " ●" : ""}
              </button>
              <button
                aria-label={`关闭 ${tab.document.path}`}
                onClick={w.closeEditor(tab.document.path)}
              >
                ×
              </button>
            </div>
          ))}
        </div>
        {w.editor.error && !readOnlyFile && (
          <p role="alert">{describeGitError(w.editor.error)}</p>
        )}
        {w.editor.notice && <p role="status">{w.editor.notice}</p>}
        {selected && (
          <div className="button-row">
            <button
              onClick={w.saveActiveEditor}
              disabled={
                Boolean(w.editor.savingPath) ||
                selected.draft === selected.baseline
              }
            >
              保存
            </button>
            <button
              onClick={w.reloadEditor}
              disabled={Boolean(w.editor.savingPath)}
            >
              重新读取
            </button>
            <span>
              UTF-8{selected.document.text.bom ? " BOM" : ""} ·{" "}
              {selected.document.text.lineEnding}
            </span>
          </div>
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
                edit={w.editor.controller.edit}
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
      <nav className="project-file-tree" aria-label="项目文件">
        <label>
          筛选文件
          <input
            type="search"
            maxLength={256}
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
        <VirtualFileTree
          tree={tree}
          selectedPath={w.projectPath}
          openFile={w.inspectProjectFile}
        />
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

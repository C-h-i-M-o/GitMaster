import type { Workbench } from "../hooks/useWorkbench";
import { useNativeDialog } from "../hooks/useNativeDialog";
import { describeGitError } from "../ui/gitPresentation";

/** 确认框独立于抽屉可见性，隐藏文件面板时仍可处理退出保护。 */
export function EditorDialogs({ w }: { w: Workbench }) {
  const closing = useNativeDialog(
    w.editor.pendingClose !== null,
    w.editor.controller.cancelClose,
  );
  const leaving = useNativeDialog(w.editorPending, w.keepEditor);
  return (
    <>
      <dialog
        ref={closing.dialogRef}
        onCancel={closing.onCancel}
        aria-label="关闭未保存文档"
      >
        <p>文档尚未保存，请选择如何关闭。</p>
        <button onClick={w.editor.controller.cancelClose}>继续编辑</button>
        <button
          onClick={w.discardEditorTab}
          disabled={Boolean(w.editor.savingPath)}
        >
          放弃修改
        </button>
        <button
          onClick={w.editor.controller.saveAndClose}
          disabled={Boolean(w.editor.savingPath)}
        >
          保存并关闭
        </button>
      </dialog>
      <dialog
        ref={leaving.dialogRef}
        onCancel={leaving.onCancel}
        aria-label="处理未保存文档"
      >
        <p>
          {w.editorReloadPath
            ? `重新读取 ${w.editorReloadPath} 前，请保存或放弃该文档的修改。`
            : "存在未保存文档，继续前请保存或放弃修改。"}
        </p>
        {w.editor.error && (
          <p role="alert">{describeGitError(w.editor.error)}</p>
        )}
        <button onClick={w.keepEditor} disabled={w.editorLeaving}>
          继续编辑
        </button>
        <button onClick={w.discardEditor} disabled={w.editorLeaving}>
          放弃并继续
        </button>
        <button onClick={w.saveEditorAndContinue} disabled={w.editorLeaving}>
          {w.editorReloadPath ? "保存并重新读取" : "全部保存并继续"}
        </button>
      </dialog>
    </>
  );
}

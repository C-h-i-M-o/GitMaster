import {
  useMonacoEditor,
  type MonacoEditorProps,
} from "../hooks/useMonacoEditor";

/** Monaco 只呈现完整受支持文档，所有保存与草稿状态来自工作台控制器。 */
export default function FileEditor(props: MonacoEditorProps) {
  const container = useMonacoEditor(props);
  return (
    <div
      className="file-editor-screen"
      ref={container}
      aria-label="文件编辑器"
    />
  );
}

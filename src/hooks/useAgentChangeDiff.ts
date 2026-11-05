import { useEffect, useRef } from "react";
import * as monaco from "monaco-editor/editor/editor.api.js";
import { monacoTheme } from "../ui/editorTheme";
import { onAppearanceChange } from "../ui/appearanceRuntime";
import "./useMonacoEditor";

/** 懒建只读 Monaco 历史 diff，并在卸载时释放编辑器和文本模型。 */
export function useAgentChangeDiff(
  artifactId: string,
  beforeContent: string,
  afterContent: string,
) {
  const container = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = container.current;
    if (!element) return;
    const applyTheme = (): void => {
      monaco.editor.defineTheme("gitmaster", monacoTheme());
      monaco.editor.setTheme("gitmaster");
    };
    applyTheme();
    const stopTheme = onAppearanceChange(applyTheme);
    const editor = monaco.editor.createDiffEditor(element, {
      automaticLayout: true,
      originalEditable: false,
      readOnly: true,
      renderSideBySide: true,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      diffWordWrap: "on",
      renderOverviewRuler: false,
    });
    const artifact = encodeURIComponent(artifactId);
    const original = monaco.editor.createModel(
      beforeContent,
      "plaintext",
      monaco.Uri.parse(`gitmaster-agent-change:///${artifact}/before`),
    );
    const modified = monaco.editor.createModel(
      afterContent,
      "plaintext",
      monaco.Uri.parse(`gitmaster-agent-change:///${artifact}/after`),
    );
    editor.setModel({ original, modified });

    return () => {
      stopTheme();
      editor.setModel(null);
      editor.dispose();
      original.dispose();
      modified.dispose();
    };
  }, [artifactId, beforeContent, afterContent]);

  return container;
}

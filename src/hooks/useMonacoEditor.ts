import { useEffect, useRef } from "react";
import * as monaco from "monaco-editor/editor/editor.api.js";
import EditorWorker from "monaco-editor/editor/editor.worker.js?worker";
import "monaco-editor/features/codeEditor/register.js";
import "monaco-editor/editor/browser/coreCommands.js";
import "monaco-editor/features/tokenization/register.js";
import "monaco-editor/features/codicon/register.js";
import "monaco-editor/features/readOnlyMessage/register.js";
import "monaco-editor/features/find/register.js";
import "monaco-editor/features/clipboard/register.js";
import "monaco-editor/features/contextmenu/register.js";
import "monaco-editor/languages/definitions/typescript/register.js";
import "monaco-editor/languages/definitions/javascript/register.js";
import "monaco-editor/languages/definitions/python/register.js";
import "monaco-editor/languages/definitions/rust/register.js";
import "monaco-editor/languages/definitions/css/register.js";
import "monaco-editor/languages/definitions/html/register.js";
import "monaco-editor/languages/definitions/markdown/register.js";
import "monaco-editor/languages/definitions/shell/register.js";
import type { EditorTab } from "../ui/editorTabs";
import type { EditorPreferences } from "../types/settings";
import { editorModelText, editorDiskText } from "../ui/editorModelText";

(
  globalThis as typeof globalThis & { MonacoEnvironment: monaco.Environment }
).MonacoEnvironment = {
  getWorker: () => new EditorWorker(),
};

export interface MonacoEditorProps {
  repositoryId: string;
  tabs: readonly EditorTab[];
  activePath: string | null;
  preferences: EditorPreferences;
  readOnly?: boolean;
  edit(path: string, content: string): void;
  save(path: string): void;
}
interface ModelEntry {
  model: monaco.editor.ITextModel;
  subscription: monaco.IDisposable;
  view: monaco.editor.ICodeEditorViewState | null;
}

/** 多文档独立模型保留撤销与视口；关闭标签或组件释放时 dispose。 */
export function useMonacoEditor(props: MonacoEditorProps) {
  const container = useRef<HTMLDivElement>(null);
  const current = useRef(props);
  current.current = props;
  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const models = useRef(new Map<string, ModelEntry>());
  const active = useRef<string | null>(null);
  const replacing = useRef(false);
  useEffect(() => {
    if (!container.current) return;
    const view = monaco.editor.create(container.current, {
      model: null,
      automaticLayout: true,
      theme: "vs-dark",
      minimap: { enabled: false },
      detectIndentation: false,
      links: false,
      fontLigatures: false,
      scrollBeyondLastLine: false,
      renderValidationDecorations: "off",
      ...current.current.preferences,
    });
    editor.current = view;
    view.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      if (active.current) current.current.save(active.current);
    });
    return () => {
      view.dispose();
      for (const entry of models.current.values()) {
        entry.subscription.dispose();
        entry.model.dispose();
      }
      models.current.clear();
      editor.current = null;
      active.current = null;
    };
  }, [props.repositoryId]);
  useEffect(() => {
    const view = editor.current;
    if (!view) return;
    replacing.current = true;
    for (const tab of props.tabs) {
      const path = tab.document.path;
      const text = editorModelText(tab.draft);
      let entry = models.current.get(path);
      if (!entry) {
        const uri = monaco.Uri.from({
          scheme: "gitmaster",
          authority: props.repositoryId,
          path: `/${path}`,
        });
        const language =
          monaco.languages
            .getLanguages()
            .find((language) =>
              language.extensions?.some((extension) =>
                path.toLowerCase().endsWith(extension),
              ),
            )?.id ?? "plaintext";
        const model = monaco.editor.createModel(text, language, uri);
        model.setEOL(monaco.editor.EndOfLineSequence.LF);
        const subscription = model.onDidChangeContent(() => {
          if (replacing.current) return;
          const document = current.current.tabs.find(
            (item) => item.document.path === path,
          )?.document;
          if (document)
            current.current.edit(
              path,
              editorDiskText(
                model.getValue(monaco.editor.EndOfLinePreference.LF),
                document.text.lineEnding,
              ),
            );
        });
        entry = { model, subscription, view: null };
        models.current.set(path, entry);
      } else if (
        entry.model.getValue(monaco.editor.EndOfLinePreference.LF) !== text
      )
        entry.model.setValue(text);
      entry.model.updateOptions({
        tabSize: props.preferences.tabSize,
        insertSpaces: true,
      });
    }
    if (active.current !== props.activePath) {
      const previous = active.current
        ? models.current.get(active.current)
        : null;
      if (previous) previous.view = view.saveViewState();
      const next = props.activePath
        ? models.current.get(props.activePath)
        : null;
      view.setModel(next?.model ?? null);
      if (next?.view) view.restoreViewState(next.view);
      active.current = props.activePath;
    }
    for (const [path, entry] of models.current) {
      if (!props.tabs.some((tab) => tab.document.path === path)) {
        entry.subscription.dispose();
        entry.model.dispose();
        models.current.delete(path);
      }
    }
    replacing.current = false;
  }, [
    props.tabs,
    props.activePath,
    props.repositoryId,
    props.preferences.tabSize,
  ]);
  useEffect(() => {
    editor.current?.updateOptions(props.preferences);
  }, [props.preferences]);
  useEffect(() => {
    editor.current?.updateOptions({ readOnly: props.readOnly ?? false });
  }, [props.readOnly]);
  return container;
}

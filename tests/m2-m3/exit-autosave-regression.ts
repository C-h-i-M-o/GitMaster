import type { Workbench } from "../../src/hooks/useWorkbench";

interface ExitHarness {
  w: Workbench;
  readonly exits: number;
  setFail: (value: boolean) => void;
  quit: () => void;
}

declare global {
  interface Window {
    exitHarness: ExitHarness;
  }
}

/** 等待实际 Hook 状态变化，超时明确失败而不是继续使用旧闭包。 */
async function until(predicate: () => boolean, label: string): Promise<void> {
  const deadline = Date.now() + 3000;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error(`等待超时：${label}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
  }
}

/** 在浏览器夹具中执行真实工作台退出流程，后端仅由官方 IPC mock 隔离。 */
export async function runExitRegression(): Promise<void> {
  const output = document.getElementById("exit-results");
  if (!output) throw new Error("缺少回归结果区域");
  const results: string[] = [];
  const h = (): ExitHarness => window.exitHarness;
  const check = (value: boolean, label: string): void => {
    if (!value) throw new Error(label);
    results.push(`通过：${label}`);
    output.textContent = results.join("\n");
  };
  try {
    await until(() => h().w.appSettings.editable, "设置可用");
    h().w.appSettings.edit((s) => ({
      ...s,
      uiPreferences: { ...s.uiPreferences, showLabels: false },
    }));
    h().quit();
    await until(
      () => h().exits === 1 || h().w.settingsPending !== null,
      "即时保存后退出",
    );
    check(
      h().exits === 1 &&
        !h().w.appSettings.dirty &&
        h().w.appSettings.saved?.settings.uiPreferences.showLabels === false,
      "有效设置在退出前自动保存，不弹应用确认",
    );
    h().setFail(true);
    h().w.appSettings.edit((s) => ({
      ...s,
      uiPreferences: { ...s.uiPreferences, elasticity: 7 },
    }));
    h().quit();
    await until(() => h().w.settingsPending === "exit", "设置失败保护");
    check(
      h().exits === 1 && h().w.appSettings.dirty,
      "设置写入失败阻止退出并保留输入",
    );
    h().w.keepSettings();
    await until(() => h().w.settingsPending === null, "取消退出");
    check(
      h().w.modal === "settings" &&
        h().w.appSettings.draft.uiPreferences.elasticity === 7,
      "取消退出回到设置并保留失败输入",
    );
    h().quit();
    await until(() => h().w.settingsPending === "exit", "再次退出保护");
    h().setFail(false);
    await h().w.saveSettingsAndContinue();
    await until(
      () => h().exits === 2 && !h().w.appSettings.dirty,
      "重试成功继续退出",
    );
    check(
      !h().w.appSettings.dirty &&
        h().w.appSettings.saved?.settings.uiPreferences.elasticity === 7,
      "显式重试成功后才继续退出",
    );

    await h().w.repo.open("/test");
    await until(() => h().w.conflicts.conflicts !== null, "冲突列表");
    await h().w.conflicts.selectFile("c");
    h().w.conflicts.edit("冲突草稿");
    h().w.openDrawer("files")();
    await until(
      () => Boolean(h().w.projectFiles?.files.length),
      "可编辑文件能力",
    );
    await h().w.editor.controller.open("a.ts");
    h().w.editor.controller.edit("a.ts", "文件草稿");
    h().setFail(true);
    h().w.appSettings.edit(
      (s) => ({
        ...s,
        uiPreferences: { ...s.uiPreferences, showLabels: true },
      }),
      true,
    );
    await until(
      () =>
        h().w.appSettings.error !== null &&
        h().w.editor.dirty &&
        h().w.conflicts.dirty,
      "三份未保存内容",
    );
    h().quit();
    await until(() => h().w.editorPending, "文件优先保护");
    check(
      !h().w.discardPending &&
        h().w.settingsPending === null &&
        h().exits === 2,
      "文件、冲突和设置并存时首先处理文件",
    );
    h().w.keepEditor();
    await until(() => !h().w.editorPending, "取消文件退出");
    check(
      h().w.editor.dirty &&
        h().w.conflicts.dirty &&
        h().w.appSettings.dirty &&
        h().exits === 2,
      "取消第一层不丢失任何草稿",
    );
    h().quit();
    await until(() => h().w.editorPending, "再次文件保护");
    await h().w.discardEditor();
    await until(() => h().w.discardPending, "冲突保护");
    check(
      h().w.conflicts.dirty &&
        h().w.settingsPending === null &&
        h().exits === 2,
      "处理文件后继续拦截冲突草稿",
    );
    h().w.keepDraft();
    await until(() => !h().w.discardPending, "取消冲突退出");
    check(
      h().w.conflicts.dirty && h().w.appSettings.dirty && h().exits === 2,
      "取消冲突层保留冲突和设置",
    );
    h().quit();
    await until(() => h().w.discardPending, "再次冲突保护");
    h().w.discardDraft();
    await until(() => h().w.settingsPending === "exit", "冲突后的设置保护");
    check(
      !h().w.conflicts.dirty && h().w.appSettings.dirty && h().exits === 2,
      "明确处理冲突后仍须等待失败设置",
    );
    h().setFail(false);
    await h().w.saveSettingsAndContinue();
    await until(
      () => h().exits === 3 && !h().w.appSettings.dirty,
      "完整顺序结束",
    );
    check(
      !h().w.editor.dirty && !h().w.conflicts.dirty && !h().w.appSettings.dirty,
      "三层处理完成后才执行最终退出动作",
    );

    await h().w.conflicts.selectFile("c");
    h().w.conflicts.edit("失效上下文草稿");
    await until(() => h().w.conflicts.dirty, "失效场景草稿");
    h().quit();
    await until(() => h().w.discardPending, "失效前退出保护");
    h().w.repo.clear();
    await until(
      () => h().w.repo.repository === null && h().w.conflicts.stale,
      "仓库上下文失效",
    );
    h().w.discardDraft();
    await until(() => !h().w.discardPending, "结束失效确认");
    check(
      h().w.conflicts.dirty && h().exits === 3,
      "仓库上下文失效不得绕过尚未处理的冲突草稿",
    );
    output.textContent = `${results.join("\n")}\n完成：${results.length} 项`;
  } catch (error: unknown) {
    output.textContent = `${results.join("\n")}\n失败：${error instanceof Error ? error.message : String(error)}`;
  }
}

import type { Workbench } from "../hooks/useWorkbench";
import { useSettingsForm } from "../hooks/useSettingsForm";
import { describeGitError } from "../ui/gitPresentation";
import type { SettingsCategory } from "../types/settings";
import { settingsFieldLocation } from "../ui/settingsField";

const categories: { id: SettingsCategory; label: string }[] = [
  { id: "general", label: "Git 环境" },
  { id: "appearance", label: "外观" },
  { id: "logging", label: "诊断日志" },
  { id: "terminal", label: "终端" },
  { id: "editor", label: "文件编辑器" },
  { id: "externalOpen", label: "外部打开" },
];

/** 六类设置共享草稿与应用入口，输入控件只负责呈现。 */
export function SettingsPanel({ w }: { w: Workbench }) {
  const s = w.appSettings;
  const f = useSettingsForm(s, w.settingsCategory, w.selectSettings);
  const d = s.draft;
  return (
    <div className="settings-layout" ref={f.form}>
      <nav aria-label="设置分类">
        {categories.map((category) => (
          <button
            key={category.id}
            className={w.settingsCategory === category.id ? "active" : ""}
            onClick={w.selectSettings(category.id)}
          >
            {category.label}
          </button>
        ))}
      </nav>
      <div className="settings-content">
        {w.preview && <p role="status">请在桌面应用中配置设置。</p>}
        {s.activity === "loading" && <p role="status">正在读取设置…</p>}
        <fieldset
          className="settings-fields"
          disabled={
            !s.editable || w.operations.busy || w.git.status === "loading"
          }
        >
          {w.settingsCategory === "general" && (
            <section>
              <h3>Git 环境</h3>
              <label>
                Git 可执行文件
                <input
                  data-settings-field="gitPath"
                  value={d.gitPath ?? ""}
                  onChange={f.gitPath}
                  placeholder="留空自动检测系统 Git"
                  spellCheck={false}
                />
              </label>
              <div className="button-row">
                <button className="secondary" onClick={f.chooseGit}>
                  选择 Git
                </button>
                <button className="secondary" onClick={w.git.refresh}>
                  重新检测
                </button>
                <button className="secondary" onClick={w.install}>
                  安装官网
                </button>
              </div>
              <p className="muted small">
                候选路径将在应用时验证。更换 Git 后需要重新打开项目。
              </p>
              {w.git.environment?.status === "ready" && (
                <div className="git-ready">
                  <strong>Git {w.git.environment.version}</strong>
                  <code>{w.git.environment.executablePath}</code>
                  <small>
                    {w.git.environment.source === "manual"
                      ? "手动设置"
                      : "自动检测"}
                  </small>
                </div>
              )}
              {w.git.status === "loading" && <p role="status">正在检测 Git…</p>}
              {w.git.error && (
                <p role="alert">{describeGitError(w.git.error)}</p>
              )}
              {f.pickerError && <p role="alert">{f.pickerError}</p>}
            </section>
          )}
          {w.settingsCategory === "appearance" && (
            <section>
              <h3>提交图</h3>
              <label>
                节点弹性：{d.uiPreferences.elasticity}
                <input
                  type="range"
                  data-settings-field="uiPreferences.elasticity"
                  min={1}
                  max={10}
                  value={d.uiPreferences.elasticity}
                  onChange={f.elasticity}
                />
              </label>
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={d.uiPreferences.showLabels}
                  onChange={f.labels}
                />
                显示提交说明（分支头始终显示）
              </label>
              <p className="muted small">遵循系统“减少动态效果”设置。</p>
            </section>
          )}
          {w.settingsCategory === "logging" && (
            <section>
              <h3>诊断日志</h3>
              <label>
                日志级别
                <select value={d.logLevel ?? "auto"} onChange={f.logLevel}>
                  <option value="auto">自动</option>
                  <option value="error">错误</option>
                  <option value="warn">警告</option>
                  <option value="info">信息</option>
                  <option value="debug">调试</option>
                  <option value="trace">详细追踪</option>
                </select>
              </label>
              <p>
                当前生效：{w.logSettings.settings?.effectiveLevel ?? "尚未读取"}
              </p>
              <p className="settings-path">
                {w.logSettings.settings?.directory}
              </p>
              <button
                className="secondary"
                disabled={w.logSettings.busy}
                onClick={w.logSettings.openDirectory}
              >
                打开日志目录
              </button>
              {w.logSettings.error && (
                <p role="alert">{describeGitError(w.logSettings.error)}</p>
              )}
            </section>
          )}
          {w.settingsCategory === "terminal" && (
            <section data-settings-field="terminal.profiles" tabIndex={-1}>
              <h3>终端</h3>
              <label>
                默认配置
                <select
                  data-settings-field="terminal.defaultProfileId"
                  value={d.terminal.defaultProfileId}
                  onChange={f.defaultProfile}
                >
                  {d.terminal.profiles.map((p) => (
                    <option key={p.profileId} value={p.profileId}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
              {d.terminal.profiles.map((p, profileIndex) => (
                <fieldset
                  className="terminal-profile"
                  key={p.profileId}
                  data-settings-field={`terminal.profiles.${profileIndex}.profileId`}
                  tabIndex={-1}
                >
                  <legend>{p.name || "未命名配置"}</legend>
                  <label>
                    名称
                    <input
                      data-settings-field={`terminal.profiles.${profileIndex}.name`}
                      value={p.name}
                      onChange={f.profileField(p.profileId, "name")}
                    />
                  </label>
                  <label>
                    Shell 程序路径
                    <input
                      data-settings-field={`terminal.profiles.${profileIndex}.executablePath`}
                      value={p.executablePath ?? ""}
                      onChange={f.profileField(p.profileId, "executablePath")}
                      placeholder="留空使用系统默认 Shell"
                      spellCheck={false}
                    />
                  </label>
                  <label>
                    启动目录
                    <select
                      value={p.cwd.kind}
                      onChange={f.profileField(p.profileId, "directory")}
                    >
                      <option value="project">当前项目</option>
                      <option value="home">用户主目录</option>
                      <option value="fixed">指定目录</option>
                    </select>
                  </label>
                  {p.cwd.kind === "fixed" && (
                    <label>
                      指定目录
                      <input
                        data-settings-field={`terminal.profiles.${profileIndex}.cwd`}
                        value={p.cwd.path}
                        onChange={f.profileField(p.profileId, "path")}
                      />
                    </label>
                  )}
                  {p.args.map((arg, index) => (
                    <div className="input-action" key={index}>
                      <label>
                        参数 {index + 1}
                        <input
                          data-settings-field={
                            index === 0
                              ? `terminal.profiles.${profileIndex}.args`
                              : undefined
                          }
                          value={arg}
                          onChange={f.argument(p.profileId, index)}
                          spellCheck={false}
                        />
                      </label>
                      <button
                        className="secondary"
                        onClick={f.removeArgument(p.profileId, index)}
                      >
                        删除参数 {index + 1}
                      </button>
                    </div>
                  ))}
                  <div className="button-row">
                    <button
                      className="secondary"
                      data-settings-field={
                        p.args.length === 0
                          ? `terminal.profiles.${profileIndex}.args`
                          : undefined
                      }
                      disabled={p.args.length >= 64}
                      onClick={f.addArgument(p.profileId)}
                    >
                      添加参数
                    </button>
                    <button
                      className="secondary"
                      disabled={
                        d.terminal.profiles.length === 1 ||
                        d.terminal.defaultProfileId === p.profileId
                      }
                      onClick={f.removeProfile(p.profileId)}
                    >
                      删除配置
                    </button>
                  </div>
                  {d.terminal.defaultProfileId === p.profileId && (
                    <p className="muted small">
                      删除此配置前，请先选择其他默认配置。
                    </p>
                  )}
                </fieldset>
              ))}
              <button
                className="secondary"
                disabled={d.terminal.profiles.length >= 32}
                onClick={f.addProfile}
              >
                新增终端配置
              </button>
              <label>
                字体
                <input
                  data-settings-field="terminal.fontFamily"
                  value={d.terminal.fontFamily}
                  onChange={f.displayField("terminal", "fontFamily")}
                />
              </label>
              <label>
                字号
                <input
                  type="number"
                  min={10}
                  max={32}
                  value={d.terminal.fontSize}
                  data-settings-field="terminal.fontSize"
                  onChange={f.displayField("terminal", "fontSize")}
                />
              </label>
              <label>
                光标
                <select value={d.terminal.cursorStyle} onChange={f.cursorStyle}>
                  <option value="block">方块</option>
                  <option value="bar">竖线</option>
                  <option value="underline">下划线</option>
                </select>
              </label>
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={d.terminal.cursorBlink}
                  onChange={f.cursorBlink}
                />
                光标闪烁
              </label>
              <label>
                回滚行数
                <input
                  type="number"
                  min={1000}
                  max={50000}
                  value={d.terminal.scrollbackLines}
                  data-settings-field="terminal.scrollbackLines"
                  onChange={f.displayField("terminal", "scrollbackLines")}
                />
              </label>
              <p className="muted small">
                每项参数独立传入，无需为包含空格的参数加引号。程序、参数和目录仅对新会话生效，保存不会启动或重启终端。
              </p>
            </section>
          )}
          {w.settingsCategory === "editor" && (
            <section>
              <h3>文件编辑器</h3>
              <label>
                字体
                <input
                  value={d.editor.fontFamily}
                  data-settings-field="editor.fontFamily"
                  onChange={f.displayField("editor", "fontFamily")}
                />
              </label>
              <label>
                字号
                <input
                  type="number"
                  min={10}
                  max={32}
                  value={d.editor.fontSize}
                  data-settings-field="editor.fontSize"
                  onChange={f.displayField("editor", "fontSize")}
                />
              </label>
              <label>
                Tab 宽度
                <select
                  data-settings-field="editor.tabSize"
                  value={d.editor.tabSize}
                  onChange={f.tabSize}
                >
                  <option value="2">2</option>
                  <option value="4">4</option>
                  <option value="8">8</option>
                </select>
              </label>
              <label>
                自动折行
                <select value={d.editor.wordWrap} onChange={f.wordWrap}>
                  <option value="off">关闭</option>
                  <option value="on">开启</option>
                </select>
              </label>
              <p className="muted small">
                显示设置不转换原文件编码或换行，文件使用手动保存。
              </p>
            </section>
          )}
          {w.settingsCategory === "externalOpen" && (
            <section>
              <h3>外部打开</h3>
              <label>
                默认打开方式
                <select
                  value={d.externalOpen.defaultAppId}
                  onChange={f.externalApp}
                >
                  <option value="fileManager">系统文件管理器</option>
                  <option value="vsCode">VS Code</option>
                  <option value="terminal">系统终端</option>
                </select>
              </label>
              <p className="muted small">
                打开目标为当前项目文件夹，不改变系统文件关联。使用时检查软件是否可用。
              </p>
            </section>
          )}
        </fieldset>
        {s.error && (
          <div role="alert">
            <p>
              {s.error.code === "STALE_SETTINGS"
                ? "设置已被其他操作修改。草稿已保留，请重新读取后核对。"
                : "设置未保存，请检查输入或重试。"}
            </p>
            {s.error.fieldErrors.map((error) => (
              <p key={error.field}>
                {settingsFieldLocation(error.field) && (
                  <button
                    className="secondary"
                    onClick={f.locateError(error.field)}
                  >
                    {settingsFieldLocation(error.field)?.label} · 定位
                  </button>
                )}{" "}
                {error.message}
              </p>
            ))}
          </div>
        )}
        <div className="settings-actions">
          <span role="status">
            {s.activity === "saving"
              ? "正在保存…"
              : s.dirty
                ? "有未应用的更改"
                : s.saved
                  ? "与已保存设置一致"
                  : "设置尚未加载"}
          </span>
          <div className="button-row">
            <button
              className="secondary"
              disabled={!s.editable}
              onClick={w.restoreSettings}
            >
              恢复当前分类默认
            </button>
            <button
              className="secondary"
              disabled={s.activity !== "idle"}
              onClick={w.reloadSettings}
            >
              重新读取
            </button>
            <button
              className="secondary"
              disabled={s.activity !== "idle"}
              onClick={w.closeModal}
            >
              取消
            </button>
            <button
              className="primary"
              disabled={
                !s.editable ||
                !s.dirty ||
                w.operations.busy ||
                w.git.status === "loading"
              }
              onClick={w.applySettings}
            >
              应用
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

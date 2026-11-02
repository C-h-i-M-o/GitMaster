import type { useAppSettings } from "../hooks/useAppSettings";
import type { useSettingsForm } from "../hooks/useSettingsForm";
import { themeCatalog, themePreview } from "../ui/themes";
import { SettingsFieldError } from "./SettingsFieldError";

const modes = [
  { id: "system", label: "跟随系统" },
  { id: "light", label: "浅色" },
  { id: "dark", label: "深色" },
] as const;

/** 模式与两个主题独立选择，预览使用对应的实际配色。 */
export function AppearanceSettingsPanel({
  settings,
  form,
}: {
  settings: ReturnType<typeof useAppSettings>;
  form: ReturnType<typeof useSettingsForm>;
}) {
  const prefs = settings.draft.uiPreferences;
  return (
    <section className="appearance-settings">
      <h3>外观</h3>
      <fieldset className="appearance-mode">
        <legend>显示模式</legend>
        <div className="appearance-mode-options">
          {modes.map((mode) => (
            <label
              key={mode.id}
              className={prefs.colorMode === mode.id ? "selected" : ""}
            >
              <input
                type="radio"
                name="colorMode"
                value={mode.id}
                checked={prefs.colorMode === mode.id}
                onChange={form.colorMode}
                data-settings-field="uiPreferences.colorMode"
              />
              {mode.label}
            </label>
          ))}
        </div>
        <SettingsFieldError
          error={settings.error}
          field="uiPreferences.colorMode"
        />
      </fieldset>
      <div className="theme-pickers">
        <div className="theme-picker">
          <div
            className="theme-sample"
            style={themePreview("light", prefs.lightTheme)}
            aria-hidden="true"
          >
            <span>Aa</span>
            <i />
            <i />
            <i />
          </div>
          <label>
            浅色主题
            <select
              value={prefs.lightTheme}
              onChange={form.lightTheme}
              data-settings-field="uiPreferences.lightTheme"
            >
              {themeCatalog
                .filter((theme) => theme.light !== null)
                .map((theme) => (
                  <option key={theme.id} value={theme.id}>
                    {theme.name}
                  </option>
                ))}
            </select>
          </label>
          <SettingsFieldError
            error={settings.error}
            field="uiPreferences.lightTheme"
          />
        </div>
        <div className="theme-picker">
          <div
            className="theme-sample"
            style={themePreview("dark", prefs.darkTheme)}
            aria-hidden="true"
          >
            <span>Aa</span>
            <i />
            <i />
            <i />
          </div>
          <label>
            深色主题
            <select
              value={prefs.darkTheme}
              onChange={form.darkTheme}
              data-settings-field="uiPreferences.darkTheme"
            >
              {themeCatalog.map((theme) => (
                <option key={theme.id} value={theme.id}>
                  {theme.name}
                </option>
              ))}
            </select>
          </label>
          <SettingsFieldError
            error={settings.error}
            field="uiPreferences.darkTheme"
          />
        </div>
      </div>
      <p className="muted small">跟随系统时，自动使用对应的浅色或深色主题。</p>
    </section>
  );
}

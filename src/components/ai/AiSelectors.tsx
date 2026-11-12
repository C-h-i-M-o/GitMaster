import type { AiWorkspaceController } from "../../hooks/useAiWorkspace";
import {
  useAiModelSelector,
  useAiPermissionSelector,
} from "../../hooks/useAiSelectors";
import { Icon } from "../Icon";
import { AiPermissionIcon } from "./AiPermissionIcon";
import modelCss from "./AiModelSelect.module.css";
import permissionCss from "./AiPermissionSelect.module.css";

/** 使用上游模型菜单样式，展示分组搜索与尾部选择标记。 */
export function AiModelSelect({ f }: { f: AiWorkspaceController }) {
  const m = useAiModelSelector(f);
  return (
    <div
      className={`${modelCss.root} ai-select-root`}
      ref={m.root}
      onKeyDown={m.keyDown}
      onBlur={m.blur}
    >
      <button
        ref={m.trigger}
        className={modelCss.trigger}
        aria-label="选择模型"
        aria-haspopup="menu"
        aria-expanded={m.open}
        onClick={m.toggle}
        disabled={f.busy || f.preview}
      >
        <span className={modelCss.triggerLabel}>{m.label}</span>
        <span className={modelCss.chevron}>
          <Icon name="chevron" />
        </span>
      </button>
      {m.open && (
        <div className="ai-select-menu" role="menu" aria-label="模型目录">
          <div className={modelCss.searchRow}>
            <label className={`${modelCss.search} ai-model-search`}>
              <Icon name="search" />
              <input
                value={m.query}
                onChange={m.search}
                placeholder="搜索模型"
                aria-label="搜索模型"
              />
            </label>
          </div>
          <div className={modelCss.groups}>
            {m.groups.length ? (
              m.groups.map((group) => (
                <section key={group.name}>
                  <div className="ai-menu-heading">{group.name}</div>
                  {group.options.map((option) => (
                    <button
                      role="menuitemradio"
                      aria-checked={option.value === f.modelValue}
                      key={option.value}
                      className={modelCss.option}
                      onClick={m.choose(option.value)}
                    >
                      <span className={modelCss.optionCopy}>
                        <span className={modelCss.modelName}>
                          {option.label}
                        </span>
                      </span>
                      {option.value === f.modelValue && (
                        <span className={modelCss.check}>
                          <Icon name="check" />
                        </span>
                      )}
                    </button>
                  ))}
                </section>
              ))
            ) : (
              <div className={modelCss.empty}>没有匹配的模型</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** 权限菜单保留紧凑触发器和一次性选择行为。 */
export function AiPermissionSelect({ f }: { f: AiWorkspaceController }) {
  const m = useAiPermissionSelector(f);
  return (
    <div
      className="ai-select-root"
      ref={m.root}
      onKeyDown={m.keyDown}
      onBlur={m.blur}
    >
      <button
        ref={m.trigger}
        className={permissionCss.trigger}
        aria-label="选择 AI 权限"
        aria-haspopup="menu"
        aria-expanded={m.open}
        onClick={m.toggle}
        disabled={f.busy}
      >
        <span className={permissionCss.triggerIcon}>
          <AiPermissionIcon value={f.permissionChoice} />
        </span>
        <span className={permissionCss.triggerLabel}>{m.label}</span>
        <span className={permissionCss.chevron}>
          <Icon name="chevron" />
        </span>
      </button>
      {m.open && (
        <div
          className="ai-select-menu ai-permission-menu"
          role="menu"
          aria-label="权限预设"
        >
          {m.options.map((option) => (
            <button
              role="menuitemradio"
              aria-checked={option.value === f.permissionChoice}
              key={option.value}
              className="ai-permission-option"
              onClick={m.choose(option.value)}
              title={option.description}
            >
              <span className="ai-permission-label">
                <AiPermissionIcon value={option.value} />
                {option.label}
              </span>
              {option.value === f.permissionChoice && <Icon name="check" />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

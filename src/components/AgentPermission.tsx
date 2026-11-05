import type { ChangeEventHandler } from "react";
import type { PermissionMode } from "../types/agent";
import { permissionOptions } from "../hooks/useAgentSettings";
import { permissionPresentation } from "../ui/agentPresentation";
import { Icon } from "./Icon";

/** 在本轮状态中展示后端记录的权限模式及其现有说明。 */
export function AgentPermissionBadge({ mode }: { mode: PermissionMode }) {
  const permission = permissionPresentation(mode, permissionOptions);
  return (
    <div className="agent-permission-badge" title={permission.description}>
      <Icon name={permission.icon} />
      <span>
        <strong>{permission.label}</strong>
        <small>{permission.description}</small>
      </span>
    </div>
  );
}

/** 展示当前默认权限选项、图标和具体允许范围说明。 */
export function AgentPermissionControl({
  mode,
  disabled,
  onChange,
}: {
  mode: PermissionMode;
  disabled: boolean;
  onChange: ChangeEventHandler<HTMLSelectElement>;
}) {
  const permission = permissionPresentation(mode, permissionOptions);
  return (
    <div className="agent-permission-control">
      <Icon name={permission.icon} />
      <label>
        <span>权限模式</span>
        <select
          aria-label="AI 权限"
          value={mode}
          onChange={onChange}
          disabled={disabled}
        >
          {permissionOptions.map((option) => (
            <option value={option.value} key={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <small>{permission.description}</small>
      </label>
    </div>
  );
}

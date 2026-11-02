import type { ReactNode } from "react";

/** 原生折叠保留内部草稿与控件，低频选项在当前面板默认收起。 */
export function SettingsAdvanced({
  description,
  children,
}: {
  description: string;
  children: ReactNode;
}) {
  return (
    <details className="settings-advanced">
      <summary>
        <span>高级选项</span>
        <small>{description}</small>
      </summary>
      <div className="settings-advanced-content">{children}</div>
    </details>
  );
}

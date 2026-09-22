import type { ReactNode } from "react";

/** 提供统一的 24px 线性图标，仅负责展示与可访问性标记。 */
export type IconName =
  | "branch"
  | "history"
  | "changes"
  | "files"
  | "download"
  | "upload"
  | "refresh"
  | "chevron"
  | "close"
  | "settings"
  | "search"
  | "plus"
  | "minus"
  | "center"
  | "folder"
  | "check"
  | "activity"
  | "merge";

const iconPaths: Record<IconName, ReactNode> = {
  branch: (
    <>
      <circle cx="6" cy="5" r="2.5" />
      <circle cx="6" cy="19" r="2.5" />
      <circle cx="18" cy="6" r="2.5" />
      <path d="M6 7.5v9M18 8.5v1a7 7 0 0 1-7 7H6" />
    </>
  ),
  history: (
    <>
      <path d="M4 12a8 8 0 1 0 2.3-5.7" />
      <path d="M4 5v4h4m4-1v4l3 2" />
    </>
  ),
  changes: (
    <>
      <path d="M5 4h14v16H5z" />
      <path d="M8 8h8M8 12h8M8 16h5" />
    </>
  ),
  files: (
    <>
      <path d="M6 3h8l4 4v14H6z" />
      <path d="M14 3v5h5M9 13h6m-6 4h6" />
    </>
  ),
  download: <path d="M12 4v11m-4-4 4 4 4-4M5 20h14" />,
  upload: <path d="M12 20V9m-4 4 4-4 4 4M5 4h14" />,
  refresh: (
    <path d="M20 11a8 8 0 0 0-14.7-4L3 10m0-5v5h5M4 13a8 8 0 0 0 14.7 4L21 14m0 5v-5h-5" />
  ),
  chevron: <path d="m8 10 4 4 4-4" />,
  close: <path d="m6 6 12 12M6 18 18 6" />,
  settings: (
    <>
      <path d="m9.7 3-.5 2.1-1.7 1-2.1-.6-2.3 4 1.6 1.5v2l-1.6 1.5 2.3 4 2.1-.6 1.7 1 .5 2.1h4.6l.5-2.1 1.7-1 2.1.6 2.3-4-1.6-1.5v-2l1.6-1.5-2.3-4-2.1.6-1.7-1-.5-2.1Z" />
      <circle cx="12" cy="12" r="3.2" />
    </>
  ),
  search: (
    <>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="m15 15 5 5" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  minus: <path d="M5 12h14" />,
  center: (
    <>
      <path d="M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5" />
      <circle cx="12" cy="12" r="3" />
    </>
  ),
  folder: <path d="M3 7V5h6l2 3h10v12H3V7Z" />,
  check: <path d="m5 12 4 4L19 6" />,
  activity: <path d="M3 12h4l2-6 4 12 2-6h6" />,
  merge: (
    <>
      <circle cx="6" cy="6" r="2.5" />
      <circle cx="6" cy="18" r="2.5" />
      <circle cx="18" cy="12" r="2.5" />
      <path d="M8.5 6h2a7.5 7.5 0 0 1 7.5 7.5M8.5 18h2A7.5 7.5 0 0 0 18 10.5" />
    </>
  ),
};

/** 渲染指定名称的统一线性 SVG 图标。 */
export function Icon({
  name,
  className,
}: {
  name: IconName;
  className?: string;
}): ReactNode {
  return (
    <svg
      className={className}
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.65"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {iconPaths[name]}
    </svg>
  );
}

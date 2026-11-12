import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
} from "react";
import type { AiWorkspaceController } from "./useAiWorkspace";
import type { AiPermission } from "../types/ai";

export const aiPermissionOptions: {
  value: AiPermission;
  label: string;
  description: string;
}[] = [
  {
    value: "read-only",
    label: "只读",
    description: "需要写入或扩大权限时询问批准",
  },
  {
    value: "workspace-write",
    label: "工作区可写",
    description: "允许工作区写入，扩大范围时询问批准",
  },
  {
    value: "danger-full-access",
    label: "完全访问",
    description: "访问当前账户可用文件与命令，不使用文件沙箱",
  },
];

/** 为 AI 两个菜单提供相同的关闭、键盘与焦点回收行为。 */
function useAiMenu() {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  /** 显式关闭后将键盘焦点还给触发按钮。 */
  function close(): void {
    setOpen(false);
    trigger.current?.focus();
  }
  /** 触发按钮切换菜单，不影响消息输入草稿。 */
  function toggle(): void {
    setOpen((old) => !old);
  }
  useEffect(() => {
    if (!open) return;
    root.current
      ?.querySelector<HTMLElement>("[role=menu] input, [role=menu] button")
      ?.focus();
    /** 外部点击只关闭当前菜单。 */
    function outside(event: PointerEvent): void {
      if (event.target instanceof Node && !root.current?.contains(event.target))
        setOpen(false);
    }
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  /** 箭头遍历菜单项，Escape 关闭，Tab 保留浏览器正常焦点顺序。 */
  function keyDown(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    }
    if (!open || !["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key))
      return;
    const items = [
      ...(root.current?.querySelectorAll<HTMLButtonElement>(
        "[role=menu] button:not(:disabled)",
      ) ?? []),
    ];
    if (!items.length) return;
    event.preventDefault();
    const index = items.findIndex((item) => item === document.activeElement);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : (index + (event.key === "ArrowUp" ? -1 : 1) + items.length) %
            items.length;
    items[next]?.focus();
  }
  /** 焦点完全离开菜单后收起浮层。 */
  function blur(event: React.FocusEvent<HTMLDivElement>): void {
    if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
  }
  return { open, root, trigger, close, toggle, keyDown, blur };
}

/** 模型菜单按提供商分组搜索，选择仍由会话控制器持久化。 */
export function useAiModelSelector(f: AiWorkspaceController) {
  const menu = useAiMenu();
  const [query, setQuery] = useState("");
  const filtered = f.options.filter((option) =>
    `${option.group} ${option.label}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const groups = [...new Set(filtered.map((option) => option.group))].map(
    (name) => ({
      name,
      options: filtered.filter((option) => option.group === name),
    }),
  );
  /** 搜索仅影响当前展示目录。 */
  function search(event: ChangeEvent<HTMLInputElement>): void {
    setQuery(event.target.value);
  }
  /** 关闭后提交确切的提供商和模型，不按显示名称猜路由。 */
  function choose(value: string): () => void {
    return () => {
      menu.close();
      void f.chooseModel(value);
    };
  }
  return {
    ...menu,
    query,
    search,
    groups,
    label:
      f.options.find((option) => option.value === f.modelValue)?.label ??
      (f.modelValue ? "模型不可用" : "选择模型"),
    choose,
  };
}

/** 权限菜单的完全访问选项仍经过上游式风险确认。 */
export function useAiPermissionSelector(f: AiWorkspaceController) {
  const menu = useAiMenu();
  /** 只把固定预设交给控制器，不携带项目内容授权。 */
  function choose(value: AiPermission): () => void {
    return () => {
      menu.close();
      f.choosePermission(value);
    };
  }
  return {
    ...menu,
    choose,
    options: aiPermissionOptions,
    label: aiPermissionOptions.find(
      (option) => option.value === f.permissionChoice,
    )?.label,
  };
}

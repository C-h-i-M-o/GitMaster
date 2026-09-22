import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import type { HistoryPage } from "../types/git";
import { layoutGraph } from "../ui/graphLayout";
interface View {
  x: number;
  y: number;
  zoom: number;
}
interface Offset {
  oid: string;
  dx: number;
  dy: number;
}
interface Drag {
  pointerId: number;
  x: number;
  y: number;
  view: View;
  oid: string | null;
  moved: boolean;
}
const COLORS = ["#378e80", "#6f9cb5", "#b69b65", "#9f82ad", "#8fa569"];
/** 图形通道颜色只表达几何连线，不推测提交所属分支。 */
export function laneColor(lane: number): string {
  return COLORS[lane % COLORS.length] ?? COLORS[0]!;
}
/** 管理画布几何交互，所有位移只影响展示，不修改提交与引用。 */
export function useCommitGraph(
  page: HistoryPage | null,
  selectedOid: string | null,
  onSelect: (oid: string) => Promise<void>,
  elasticity: number,
) {
  const root = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 800, height: 600 });
  const measuredSize = useRef({ width: 800, height: 600 });
  const [view, setView] = useState<View>({ x: 180, y: 20, zoom: 1 });
  const [offset, setOffset] = useState<Offset | null>(null);
  const [query, setQuery] = useState("");
  const [reduced, setReduced] = useState(false);
  const layout = useMemo(() => layoutGraph(page), [page]);
  const drag = useRef<Drag | null>(null);
  const frame = useRef<number | null>(null);
  const liveOffset = useRef<Offset | null>(null);
  const svg = useRef<SVGSVGElement>(null);
  /** 显示偏移同时写入引用，指针松开读取最后一次真实位置。 */
  const moveOffset = useCallback((next: Offset | null): void => {
    liveOffset.current = next;
    setOffset(next);
  }, []);
  /** 停止自己的弹簧动画，卸载和新拖拽均复用。 */
  const stopSpring = useCallback((): void => {
    if (frame.current !== null) cancelAnimationFrame(frame.current);
    frame.current = null;
  }, []);
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    const observer = new ResizeObserver((entries) => {
      const box = entries[0]?.contentRect;
      if (box) {
        const difference = box.width - measuredSize.current.width;
        measuredSize.current = { width: box.width, height: box.height };
        setSize(measuredSize.current);
        setView((old) => ({ ...old, x: old.x + difference / 2 }));
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    const media = matchMedia("(prefers-reduced-motion: reduce)");
    /** 同步系统减少动效偏好。 */
    const changed = (): void => setReduced(media.matches);
    changed();
    media.addEventListener("change", changed);
    return () => media.removeEventListener("change", changed);
  }, []);
  useEffect(() => {
    stopSpring();
    moveOffset(null);
    drag.current = null;
    setView({
      x: Math.max(20, measuredSize.current.width * 0.26 - 150),
      y: 20,
      zoom: 1,
    });
  }, [page?.graphSnapshotId, stopSpring, moveOffset]);
  useEffect(() => () => stopSpring(), [stopSpring]);
  /** 固定指针下的世界坐标缩放，防止缩放跳到无关位置。 */
  const zoomAt = useCallback((factor: number, x: number, y: number): void => {
    setView((old) => {
      const zoom = Math.max(0.25, Math.min(2.5, old.zoom * factor));
      const ratio = zoom / old.zoom;
      return { x: x - (x - old.x) * ratio, y: y - (y - old.y) * ratio, zoom };
    });
  }, []);
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    /** 使用非被动滚轮监听，缩放不会同时滚动父页面。 */
    const wheel = (event: WheelEvent): void => {
      event.preventDefault();
      const rect = element.getBoundingClientRect();
      zoomAt(
        Math.exp(-event.deltaY * 0.0015),
        event.clientX - rect.left,
        event.clientY - rect.top,
      );
    };
    element.addEventListener("wheel", wheel, { passive: false });
    return () => element.removeEventListener("wheel", wheel);
  }, [zoomAt]);
  /** 将选中提交或最新提交移动到画布中央。 */
  const center = useCallback((): void => {
    const node =
      layout.nodes.find((n) => n.oid === selectedOid) ?? layout.nodes[0];
    if (node)
      setView((old) => ({
        ...old,
        x: size.width / 2 - node.x * old.zoom,
        y: size.height * 0.4 - node.y * old.zoom,
      }));
  }, [layout, selectedOid, size]);
  /** 工具条放大，以当前视口中心为锚点。 */
  const zoomIn = useCallback(
    (): void => zoomAt(1.2, size.width / 2, size.height / 2),
    [zoomAt, size],
  );
  /** 工具条缩小，以当前视口中心为锚点。 */
  const zoomOut = useCallback(
    (): void => zoomAt(1 / 1.2, size.width / 2, size.height / 2),
    [zoomAt, size],
  );
  /** 开始节点拖动或背景平移，并捕获指针。 */
  const pointerDown = useCallback(
    (event: PointerEvent<SVGSVGElement>): void => {
      if (event.button !== 0 || !event.isPrimary) return;
      const target =
        event.target instanceof Element
          ? event.target.closest<SVGGElement>("[data-oid]")
          : null;
      stopSpring();
      moveOffset(null);
      drag.current = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        view,
        oid: target?.dataset.oid ?? null,
        moved: false,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [view, stopSpring, moveOffset],
  );
  /** 指针只改变当前节点偏移或视口，不移动相邻拓扑。 */
  const pointerMove = useCallback(
    (event: PointerEvent<SVGSVGElement>): void => {
      const active = drag.current;
      if (!active || active.pointerId !== event.pointerId) return;
      const dx = event.clientX - active.x,
        dy = event.clientY - active.y;
      if (Math.hypot(dx, dy) > 4) active.moved = true;
      if (active.oid)
        moveOffset({
          oid: active.oid,
          dx: dx / active.view.zoom,
          dy: dy / active.view.zoom,
        });
      else
        setView({
          ...active.view,
          x: active.view.x + dx,
          y: active.view.y + dy,
        });
    },
    [moveOffset],
  );
  /** 释放节点后回弹；减少动效时直接恢复原坐标。 */
  const release = useCallback(
    (event: PointerEvent<SVGSVGElement>): void => {
      const active = drag.current;
      if (!active || active.pointerId !== event.pointerId) return;
      drag.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId))
        event.currentTarget.releasePointerCapture(event.pointerId);
      if (active.oid && !active.moved) void onSelect(active.oid);
      const current = liveOffset.current;
      if (!current || reduced) {
        moveOffset(null);
        return;
      }
      let x = current.dx,
        y = current.dy,
        vx = 0,
        vy = 0,
        last = performance.now();
      /** 弹簧每帧只更新一个节点及相连边，收敛后停止。 */
      const step = (now: number): void => {
        const dt = Math.min(2, (now - last) / 16.667);
        last = now;
        const damping = Math.pow(
          0.76 + Math.max(1, Math.min(10, elasticity)) * 0.015,
          dt,
        );
        vx = (vx - x * 0.09 * dt) * damping;
        vy = (vy - y * 0.09 * dt) * damping;
        x += vx * dt;
        y += vy * dt;
        if (Math.abs(x) + Math.abs(y) + Math.abs(vx) + Math.abs(vy) < 0.2) {
          frame.current = null;
          moveOffset(null);
          return;
        }
        moveOffset({ oid: current.oid, dx: x, dy: y });
        frame.current = requestAnimationFrame(step);
      };
      frame.current = requestAnimationFrame(step);
    },
    [onSelect, reduced, elasticity, moveOffset],
  );
  /** 取消指针交互只清理展示位移，不触发提交选择。 */
  const cancel = useCallback((): void => {
    drag.current = null;
    stopSpring();
    moveOffset(null);
  }, [stopSpring, moveOffset]);
  /** 键盘选择同样可进入详情，并将屏幕外的下一节点带回视口。 */
  const nodeKeyDown = useCallback(
    (event: KeyboardEvent<SVGGElement>): void => {
      const oid = event.currentTarget.dataset.oid;
      if (!oid) return;
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        void onSelect(oid);
        return;
      }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      const index = layout.nodes.findIndex((n) => n.oid === oid);
      const target = layout.nodes[index + (event.key === "ArrowDown" ? 1 : -1)];
      if (!target) return;
      void onSelect(target.oid);
      setView((old) => ({
        ...old,
        x: size.width / 2 - target.x * old.zoom,
        y: size.height * 0.4 - target.y * old.zoom,
      }));
      requestAnimationFrame(() =>
        svg.current
          ?.querySelector<SVGGElement>(`[data-oid="${target.oid}"]`)
          ?.focus(),
      );
    },
    [layout, onSelect, size],
  );
  /** 过滤只改变强调程度，图中的父子连线保持真实。 */
  const changeQuery = useCallback(
    (event: ChangeEvent<HTMLInputElement>): void =>
      setQuery(event.target.value),
    [],
  );
  const positions = new Map(
    layout.nodes.map((node) => [
      node.oid,
      {
        x: node.x + (offset?.oid === node.oid ? offset.dx : 0),
        y: node.y + (offset?.oid === node.oid ? offset.dy : 0),
      },
    ]),
  );
  const bounds = {
    left: (-view.x - 300) / view.zoom,
    right: (size.width - view.x + 300) / view.zoom,
    top: (-view.y - 100) / view.zoom,
    bottom: (size.height - view.y + 100) / view.zoom,
  };
  const normalized = query.trim().toLocaleLowerCase();
  const nodes = layout.nodes
    .filter((node) => {
      const p = positions.get(node.oid)!;
      return (
        p.x >= bounds.left &&
        p.x <= bounds.right &&
        p.y >= bounds.top &&
        p.y <= bounds.bottom
      );
    })
    .map((node) => ({
      ...node,
      ...positions.get(node.oid)!,
      matches:
        !normalized ||
        [
          node.oid,
          node.commit.subject,
          node.commit.authorName,
          ...node.refs.map((ref) => ref.name),
        ].some((text) => text.toLocaleLowerCase().includes(normalized)),
    }));
  const edges = layout.edges.flatMap((edge) => {
    const start = positions.get(edge.from);
    if (!start) return [];
    const end = positions.get(edge.to) ?? { x: edge.endX, y: edge.endY };
    if (
      Math.max(start.x, end.x) < bounds.left ||
      Math.min(start.x, end.x) > bounds.right ||
      Math.max(start.y, end.y) < bounds.top ||
      Math.min(start.y, end.y) > bounds.bottom
    )
      return [];
    const middle = (start.y + end.y) / 2;
    return [
      {
        ...edge,
        path: `M ${start.x} ${start.y} C ${start.x} ${middle}, ${end.x} ${middle}, ${end.x} ${end.y}`,
      },
    ];
  });
  return {
    root,
    svg,
    nodes,
    edges,
    query,
    changeQuery,
    view,
    transform: `translate(${view.x} ${view.y}) scale(${view.zoom})`,
    center,
    zoomIn,
    zoomOut,
    pointerDown,
    pointerMove,
    release,
    cancel,
    nodeKeyDown,
  };
}

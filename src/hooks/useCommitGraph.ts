import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import type { HistoryPage } from "../types/git";
import { layoutGraph } from "../ui/graphLayout";
import { createGraphSimulation } from "../ui/graphSimulation";
interface View {
  x: number;
  y: number;
  zoom: number;
}
interface Drag {
  pointerId: number;
  x: number;
  y: number;
  lastX: number;
  lastY: number;
  time: number;
  view: View;
  oid: string | null;
  nodeX: number;
  nodeY: number;
  moved: boolean;
}
const COLORS = ["#378e80", "#6f9cb5", "#b69b65", "#9f82ad", "#8fa569"];
/** 颜色表达几何通道，不推测提交所属分支。 */
export function laneColor(lane: number): string {
  return COLORS[lane % COLORS.length] ?? COLORS[0]!;
}
/** 图形模拟在 React 外更新 SVG；React 只负责数据、选择和控件。 */
export function useCommitGraph(
  page: HistoryPage | null,
  selectedOid: string | null,
  onSelect: (oid: string) => Promise<void>,
  elasticity: number,
) {
  const root = useRef<HTMLDivElement>(null),
    svg = useRef<SVGSVGElement>(null);
  const [view, setView] = useState<View>({ x: 100, y: 20, zoom: 1 });
  const liveView = useRef(view),
    size = useRef({ width: 800, height: 600 });
  const [query, setQuery] = useState(""),
    [reduced, setReduced] = useState(false);
  const layout = useMemo(() => layoutGraph(page), [page]);
  const scene = useRef<ReturnType<typeof createGraphSimulation> | null>(null);
  const snapshot = useRef<string | undefined>(undefined),
    drag = useRef<Drag | null>(null);
  const velocity = useRef({ x: 0, y: 0 }),
    redraw = useRef<() => void>(() => {});
  /** 控件同步实时视口，动画不依赖 React 重渲染。 */
  const commitView = useCallback((next: View): void => {
    liveView.current = next;
    setView(next);
    redraw.current();
  }, []);
  useEffect(() => {
    const media = matchMedia("(prefers-reduced-motion: reduce)");
    /** 系统偏好变化即时停止持续动效。 */
    const changed = (): void => setReduced(media.matches);
    changed();
    media.addEventListener("change", changed);
    return () => media.removeEventListener("change", changed);
  }, []);
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    const observer = new ResizeObserver((entries) => {
      const box = entries[0]?.contentRect;
      if (!box) return;
      const delta = box.width - size.current.width;
      size.current = { width: box.width, height: box.height };
      commitView({ ...liveView.current, x: liveView.current.x + delta / 2 });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [commitView]);
  useLayoutEffect(() => {
    const element = svg.current;
    if (!element) return;
    const same = snapshot.current === page?.graphSnapshotId;
    const current = createGraphSimulation(
      layout,
      elasticity,
      same ? scene.current?.nodes : [],
    );
    scene.current?.stop();
    scene.current = current;
    snapshot.current = page?.graphSnapshotId;
    drag.current = null;
    velocity.current = { x: 0, y: 0 };
    if (!same)
      commitView({
        x: Math.max(35, size.current.width * 0.22 - 150),
        y: 20,
        zoom: 1,
      });
    const world = element.querySelector<SVGGElement>("[data-graph-world]");
    const nodeElements = Array.from(
      element.querySelectorAll<SVGGElement>("[data-oid]"),
    ).map((dom) => ({ dom, node: current.byId.get(dom.dataset.oid!)! }));
    const edgeElements = Array.from(
      element.querySelectorAll<SVGPathElement>("[data-edge]"),
    ).map((dom, index) => ({ dom, edge: layout.edges[index]! }));
    let frame = 0,
      last = 0;
    /** 只绘制视口附近对象，不把每帧坐标送进 React。 */
    const paint = (): void => {
      const v = liveView.current;
      world?.setAttribute(
        "transform",
        `translate(${v.x} ${v.y}) scale(${v.zoom})`,
      );
      const left = (-v.x - 330) / v.zoom,
        right = (size.current.width - v.x + 100) / v.zoom,
        top = (-v.y - 100) / v.zoom,
        bottom = (size.current.height - v.y + 100) / v.zoom;
      for (const { dom, node } of nodeElements) {
        const visible =
          node.x >= left &&
          node.x <= right &&
          node.y >= top &&
          node.y <= bottom;
        const display = visible ? "" : "none";
        if (dom.style.display !== display) dom.style.display = display;
        if (visible)
          dom.setAttribute(
            "transform",
            `translate(${node.x.toFixed(2)} ${node.y.toFixed(2)})`,
          );
      }
      for (const { dom, edge } of edgeElements) {
        const a = current.byId.get(edge.from)!,
          b = current.byId.get(edge.to) ?? { x: edge.endX, y: edge.endY };
        const visible =
          Math.max(a.x, b.x) >= left &&
          Math.min(a.x, b.x) <= right &&
          Math.max(a.y, b.y) >= top &&
          Math.min(a.y, b.y) <= bottom;
        const display = visible ? "" : "none";
        if (dom.style.display !== display) dom.style.display = display;
        if (visible) {
          const middle = (a.y + b.y) / 2;
          dom.setAttribute(
            "d",
            `M ${a.x} ${a.y} C ${a.x} ${middle}, ${b.x} ${middle}, ${b.x} ${b.y}`,
          );
        }
      }
    };
    /** 一个 RAF 驱动受力与视口惯性；后台和减少动效模式不空转。 */
    const animate = (now: number): void => {
      frame = 0;
      if (document.hidden) return;
      if (!reduced && now - last >= (drag.current ? 15 : 30)) {
        const scale = Math.min(2, (now - last) / 16.667) || 1;
        last = now;
        current.tick(now, true);
        if (
          !drag.current &&
          Math.hypot(velocity.current.x, velocity.current.y) > 0.08
        ) {
          liveView.current = {
            ...liveView.current,
            x: liveView.current.x + velocity.current.x * scale,
            y: liveView.current.y + velocity.current.y * scale,
          };
          velocity.current.x *= Math.pow(0.88, scale);
          velocity.current.y *= Math.pow(0.88, scale);
        }
        paint();
      }
      if (!reduced && current.nodes.length)
        frame = requestAnimationFrame(animate);
    };
    /** 所有外部交互共享同一绘制入口。 */
    const requestPaint = (): void => {
      paint();
      if (!frame && !reduced && !document.hidden && current.nodes.length)
        frame = requestAnimationFrame(animate);
    };
    /** 重新显示时不累计后台经过的时间。 */
    const visibility = (): void => {
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
      last = performance.now();
      if (!document.hidden) requestPaint();
    };
    if (reduced)
      for (const node of current.nodes) {
        node.x = node.anchorX;
        node.y = node.anchorY;
        node.vx = 0;
        node.vy = 0;
      }
    redraw.current = requestPaint;
    requestPaint();
    document.addEventListener("visibilitychange", visibility);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      current.stop();
      document.removeEventListener("visibilitychange", visibility);
      redraw.current = () => {};
    };
  }, [layout, page?.graphSnapshotId, elasticity, reduced, commitView]);
  /** 缩放固定指针下的图坐标并停止旧惯性。 */
  const zoomAt = useCallback(
    (factor: number, x: number, y: number): void => {
      const old = liveView.current,
        zoom = Math.max(0.25, Math.min(2.5, old.zoom * factor)),
        ratio = zoom / old.zoom;
      velocity.current = { x: 0, y: 0 };
      commitView({
        x: x - (x - old.x) * ratio,
        y: y - (y - old.y) * ratio,
        zoom,
      });
    },
    [commitView],
  );
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    /** 非被动滚轮只在画布消费，不劫持搜索输入。 */
    const wheel = (event: WheelEvent): void => {
      if (
        !(event.target instanceof Element) ||
        !event.target.closest(".commit-svg")
      )
        return;
      event.preventDefault();
      const r = element.getBoundingClientRect();
      zoomAt(
        Math.exp(-event.deltaY * 0.0015),
        event.clientX - r.left,
        event.clientY - r.top,
      );
    };
    element.addEventListener("wheel", wheel, { passive: false });
    return () => element.removeEventListener("wheel", wheel);
  }, [zoomAt]);
  /** 将选中或最新提交按实时坐标带回视口。 */
  const center = useCallback((): void => {
    const node =
      scene.current?.byId.get(selectedOid ?? "") ?? scene.current?.nodes[0];
    if (!node) return;
    velocity.current = { x: 0, y: 0 };
    commitView({
      ...liveView.current,
      x: size.current.width / 2 - node.x * liveView.current.zoom,
      y: size.current.height * 0.4 - node.y * liveView.current.zoom,
    });
  }, [selectedOid, commitView]);
  /** 视口中心放大。 */
  const zoomIn = useCallback(
    (): void => zoomAt(1.2, size.current.width / 2, size.current.height / 2),
    [zoomAt],
  );
  /** 视口中心缩小。 */
  const zoomOut = useCallback(
    (): void =>
      zoomAt(1 / 1.2, size.current.width / 2, size.current.height / 2),
    [zoomAt],
  );
  /** 捕获指针，节点跟手而邻接节点由模拟受力。 */
  const pointerDown = useCallback(
    (event: PointerEvent<SVGSVGElement>): void => {
      if (event.button !== 0 || !event.isPrimary) return;
      const target =
        event.target instanceof Element
          ? event.target.closest<SVGGElement>("[data-oid]")
          : null;
      const oid = target?.dataset.oid ?? null,
        node = scene.current?.byId.get(oid ?? "");
      drag.current = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        lastX: event.clientX,
        lastY: event.clientY,
        time: event.timeStamp,
        view: { ...liveView.current },
        oid,
        nodeX: node?.x ?? 0,
        nodeY: node?.y ?? 0,
        moved: false,
      };
      velocity.current = { x: 0, y: 0 };
      if (node) {
        node.fx = node.x;
        node.fy = node.y;
        scene.current?.wake();
      }
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [],
  );
  /** 移动只写实时模型，不触发 React 图形重渲染。 */
  const pointerMove = useCallback(
    (event: PointerEvent<SVGSVGElement>): void => {
      const active = drag.current;
      if (!active || active.pointerId !== event.pointerId) return;
      const dx = event.clientX - active.x,
        dy = event.clientY - active.y;
      if (Math.hypot(dx, dy) > 4) active.moved = true;
      const node = scene.current?.byId.get(active.oid ?? "");
      if (node) {
        node.fx = node.x = active.nodeX + dx / active.view.zoom;
        node.fy = node.y = active.nodeY + dy / active.view.zoom;
        scene.current?.wake();
      } else {
        const dt = Math.max(8, event.timeStamp - active.time);
        velocity.current = {
          x: Math.max(
            -45,
            Math.min(45, ((event.clientX - active.lastX) / dt) * 16.667),
          ),
          y: Math.max(
            -45,
            Math.min(45, ((event.clientY - active.lastY) / dt) * 16.667),
          ),
        };
        liveView.current = {
          ...active.view,
          x: active.view.x + dx,
          y: active.view.y + dy,
        };
      }
      active.lastX = event.clientX;
      active.lastY = event.clientY;
      active.time = event.timeStamp;
      redraw.current();
    },
    [],
  );
  /** 松手释放约束回稳；只有未拖动的点击进入详情。 */
  const release = useCallback(
    (event: PointerEvent<SVGSVGElement>): void => {
      const active = drag.current;
      if (!active || active.pointerId !== event.pointerId) return;
      drag.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId))
        event.currentTarget.releasePointerCapture(event.pointerId);
      const node = scene.current?.byId.get(active.oid ?? "");
      if (node) {
        node.fx = null;
        node.fy = null;
        if (reduced) {
          node.x = node.anchorX;
          node.y = node.anchorY;
        }
        scene.current?.wake();
      }
      if (reduced || event.timeStamp - active.time > 100)
        velocity.current = { x: 0, y: 0 };
      setView({ ...liveView.current });
      redraw.current();
      if (active.oid && !active.moved) void onSelect(active.oid);
    },
    [onSelect, reduced],
  );
  /** 系统取消释放固定点，不打开详情。 */
  const cancel = useCallback((): void => {
    const node = scene.current?.byId.get(drag.current?.oid ?? "");
    if (node) {
      node.fx = null;
      node.fy = null;
    }
    drag.current = null;
    velocity.current = { x: 0, y: 0 };
    redraw.current();
  }, []);
  /** 键盘导航沿提交顺序，使用当前物理坐标定位。 */
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
      const index = layout.nodes.findIndex((node) => node.oid === oid),
        target = layout.nodes[index + (event.key === "ArrowDown" ? 1 : -1)];
      if (!target) return;
      const point = scene.current?.byId.get(target.oid) ?? target;
      commitView({
        ...liveView.current,
        x: size.current.width / 2 - point.x * liveView.current.zoom,
        y: size.current.height * 0.4 - point.y * liveView.current.zoom,
      });
      svg.current
        ?.querySelector<SVGGElement>(`[data-oid="${target.oid}"]`)
        ?.focus();
      void onSelect(target.oid);
    },
    [layout, onSelect, commitView],
  );
  /** 搜索改变强调程度，不改变拓扑。 */
  const changeQuery = useCallback(
    (event: ChangeEvent<HTMLInputElement>): void =>
      setQuery(event.target.value),
    [],
  );
  const normalized = query.trim().toLocaleLowerCase();
  const nodes = useMemo(
    () =>
      layout.nodes.map((node) => ({
        ...node,
        matches:
          !normalized ||
          [
            node.oid,
            node.commit.subject,
            node.commit.authorName,
            ...node.refs.map((ref) => ref.name),
          ].some((text) => text.toLocaleLowerCase().includes(normalized)),
      })),
    [layout, normalized],
  );
  return {
    root,
    svg,
    nodes,
    edges: layout.edges,
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

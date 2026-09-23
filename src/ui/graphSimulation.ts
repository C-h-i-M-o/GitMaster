import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceCollide,
  forceX,
  forceY,
  type SimulationNodeDatum,
  type SimulationLinkDatum,
} from "d3-force";
import type { GraphLayout, GraphNode } from "./graphLayout.ts";
export interface FloatingNode extends GraphNode, SimulationNodeDatum {
  x: number;
  y: number;
  anchorX: number;
  anchorY: number;
}
interface FloatingLink extends SimulationLinkDatum<FloatingNode> {
  distance: number;
}
/** 仅处理展示受力；停止 d3 自带计时器，由画布统一调度与暂停。 */
export function createGraphSimulation(
  layout: GraphLayout,
  elasticity: number,
  previous: readonly FloatingNode[] = [],
) {
  const old = new Map(previous.map((node) => [node.oid, node]));
  const nodes: FloatingNode[] = layout.nodes.map((node) => {
    const saved = old.get(node.oid);
    return {
      ...node,
      anchorX: node.x,
      anchorY: node.y,
      x: saved?.x ?? node.x,
      y: saved?.y ?? node.y,
      vx: saved?.vx ?? 0,
      vy: saved?.vy ?? 0,
    };
  });
  const byId = new Map(nodes.map((node) => [node.oid, node]));
  const links: FloatingLink[] = layout.edges
    .filter((edge) => !edge.boundary)
    .map((edge) => {
      const from = byId.get(edge.from)!,
        to = byId.get(edge.to)!;
      return {
        source: from.oid,
        target: to.oid,
        distance:
          Math.hypot(from.anchorX - to.anchorX, from.anchorY - to.anchorY) *
          0.9,
      };
    });
  const horizontal = forceX<FloatingNode>((node) => node.anchorX).strength(
    0.014,
  );
  const vertical = forceY<FloatingNode>((node) => node.anchorY).strength(0.04);
  const simulation = forceSimulation(nodes)
    .force(
      "links",
      forceLink<FloatingNode, FloatingLink>(links)
        .id((node) => node.oid)
        .distance((link) => link.distance)
        .strength(0.14 + Math.max(1, Math.min(10, elasticity)) * 0.012),
    )
    .force(
      "charge",
      forceManyBody<FloatingNode>().strength(-105).distanceMax(320),
    )
    .force("collision", forceCollide<FloatingNode>(36).strength(0.8))
    .force("horizontal", horizontal)
    .force("vertical", vertical)
    .velocityDecay(0.29)
    .alphaDecay(0.035)
    .stop();
  return {
    nodes,
    byId,
    /** 水流移动软锚点，边与节点始终使用同一组坐标。 */
    tick(time: number, drift: boolean): void {
      if (drift) {
        horizontal.x(
          (node, index) =>
            node.anchorX + Math.sin(time / 2400 + index * 1.43) * 12,
        );
        vertical.y(
          (node, index) =>
            node.anchorY + Math.cos(time / 3100 + index * 1.17) * 7,
        );
        simulation.alpha(Math.max(simulation.alpha(), 0.09));
      }
      simulation.tick();
    },
    /** 指针施力时加热模拟，位移沿真实连接传播。 */
    wake(): void {
      simulation.alpha(0.75);
    },
    /** 清理生命周期拥有的计时资源。 */
    stop(): void {
      simulation.stop();
    },
  };
}

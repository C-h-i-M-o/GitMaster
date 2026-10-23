import type { CSSProperties } from "react";

type GraphLaneStyle = CSSProperties & {
  "--graph-color": string;
  "--graph-wash": string;
  "--graph-border": string;
};

// 深色线条和浅色标签成组维护，与纸色背景及系统青绿色协调。
const tones = [
  ["#2f7468", "#eaf3ef"],
  ["#466f96", "#edf2f8"],
  ["#8c602a", "#f8f1e6"],
  ["#80608e", "#f3eef6"],
  ["#a0546d", "#f8edf1"],
  ["#596f39", "#eff3e8"],
] as const;
const styles: readonly GraphLaneStyle[] = tones.map(([color, wash]) => ({
  "--graph-color": color,
  "--graph-wash": wash,
  "--graph-border": `${color}55`,
}));

/** 统一节点、连线和引用的通道色；共享祖先不推断唯一分支归属。 */
export function graphLaneStyle(lane: number): GraphLaneStyle {
  return styles[lane % styles.length] ?? styles[0]!;
}

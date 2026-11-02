import type { CSSProperties } from "react";

type GraphLaneStyle = CSSProperties & {
  "--graph-color": string;
  "--graph-wash": string;
  "--graph-border": string;
};

// 通道颜色来自当前主题，CSS 变量更新无需重建图形布局。
const tones = [
  "--green",
  "--info",
  "--warning",
  "--purple",
  "--danger",
  "--success",
] as const;
const styles: readonly GraphLaneStyle[] = tones.map((color) => ({
  "--graph-color": `var(${color})`,
  "--graph-wash": `color-mix(in srgb, var(${color}) 12%, var(--paper))`,
  "--graph-border": `color-mix(in srgb, var(${color}) 35%, var(--paper))`,
}));

/** 统一节点、连线和引用的通道色；共享祖先不推断唯一分支归属。 */
export function graphLaneStyle(lane: number): GraphLaneStyle {
  return styles[lane % styles.length] ?? styles[0]!;
}

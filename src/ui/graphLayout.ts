import type { CommitSummary, HistoryPage } from "../types/git.ts";

export interface GraphNode {
  oid: string;
  x: number;
  y: number;
  lane: number;
  commit: CommitSummary;
  refs: HistoryPage["tips"];
}
export interface GraphEdge {
  id: string;
  from: string;
  to: string;
  boundary: boolean;
  endX: number;
  endY: number;
}
export interface GraphLayout {
  nodes: GraphNode[];
  edges: GraphEdge[];
  width: number;
  height: number;
}

/** 按提交先子后父的稳定顺序生成可增量展示的提交图布局。 */
export function layoutGraph(page: HistoryPage | null): GraphLayout {
  if (!page) return { nodes: [], edges: [], width: 0, height: 0 };
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const lanes: Array<string | null> = [];
  const positions = new Map<string, GraphNode>();
  const refsByOid = new Map<string, HistoryPage["tips"]>();
  page.tips.forEach((tip) => {
    const refs = refsByOid.get(tip.oid) ?? [];
    refs.push(tip);
    refsByOid.set(tip.oid, refs);
  });
  page.commits.forEach((commit, index) => {
    const knownLane = lanes.indexOf(commit.oid);
    const lane =
      knownLane >= 0 ? knownLane : lanes.findIndex((value) => value === null);
    const nodeLane = lane >= 0 ? lane : lanes.length;
    if (nodeLane === lanes.length) lanes.push(null);
    lanes[nodeLane] = null;
    const node: GraphNode = {
      oid: commit.oid,
      x: 150 + nodeLane * 220,
      y: 140 + index * 112,
      lane: nodeLane,
      commit,
      refs: refsByOid.get(commit.oid) ?? [],
    };
    nodes.push(node);
    positions.set(commit.oid, node);
    commit.parentOids.forEach((parentOid, parentIndex) => {
      const parentLane = lanes.indexOf(parentOid);
      let targetLane = parentLane;
      if (targetLane < 0 && parentIndex === 0) targetLane = node.lane;
      if (targetLane < 0) {
        const empty = lanes.findIndex((value) => value === null);
        targetLane = empty >= 0 ? empty : lanes.length;
        if (targetLane === lanes.length) lanes.push(null);
      }
      lanes[targetLane] = parentOid;
      const target = positions.get(parentOid);
      edges.push({
        id: `${commit.oid}->${parentOid}:${parentIndex}`,
        from: commit.oid,
        to: parentOid,
        boundary: target === undefined,
        endX: target?.x ?? 150 + targetLane * 220,
        endY: target?.y ?? 140 + page.commits.length * 112,
      });
    });
  });
  edges.forEach((edge) => {
    const target = positions.get(edge.to);
    if (target) {
      edge.boundary = false;
      edge.endX = target.x;
      edge.endY = target.y;
    }
  });
  const laneCount = Math.max(
    1,
    lanes.length,
    ...nodes.map((node) => node.lane + 1),
  );
  return {
    nodes,
    edges,
    width: 150 + laneCount * 220,
    height: page.commits.length === 0 ? 0 : 140 + page.commits.length * 112,
  };
}

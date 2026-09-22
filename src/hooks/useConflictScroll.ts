import { useCallback, useMemo, useRef } from "react";
import type { UIEvent, RefObject } from "react";
import { alignConflictLines } from "../ui/conflictAlignment.ts";
export interface ConflictScrollState {
  localRef: RefObject<HTMLPreElement | null>;
  incomingRef: RefObject<HTMLPreElement | null>;
  resultRef: RefObject<HTMLDivElement | null>;
  onScroll: (event: UIEvent<HTMLElement>) => void;
  localLines: string[];
  incomingLines: string[];
  maxLines: number;
  resultStyle: { height: number; width: string };
}
/** 为冲突三视图提供固定行高、结果外层滚动和像素级同步滚动。 */
export function useConflictScroll(
  local: string | null,
  incoming: string | null,
  result: string,
): ConflictScrollState {
  const localRef = useRef<HTMLPreElement | null>(null);
  const incomingRef = useRef<HTMLPreElement | null>(null);
  const resultRef = useRef<HTMLDivElement | null>(null);
  const alignment = useMemo(
    () => alignConflictLines(local, incoming, result),
    [local, incoming, result],
  );
  const synchronized = useRef(
    new WeakMap<HTMLElement, { top: number; left: number }>(),
  );
  /** 忽略程序同步产生的滚动事件，短行钳制不能反向拖回源面板。 */
  const onScroll = useCallback((event: UIEvent<HTMLElement>): void => {
    const source = event.currentTarget;
    const expected = synchronized.current.get(source);
    synchronized.current.delete(source);
    if (
      expected &&
      expected.top === source.scrollTop &&
      expected.left === source.scrollLeft
    )
      return;
    [localRef.current, incomingRef.current, resultRef.current].forEach(
      (target) => {
        if (target === null || target === source) return;
        if (target.scrollTop !== source.scrollTop)
          target.scrollTop = source.scrollTop;
        if (target.scrollLeft !== source.scrollLeft)
          target.scrollLeft = source.scrollLeft;
        synchronized.current.set(target, {
          top: target.scrollTop,
          left: target.scrollLeft,
        });
      },
    );
  }, []);
  return {
    localRef,
    incomingRef,
    resultRef,
    onScroll,
    localLines: [
      ...alignment.localLines,
      ...Array<string>(alignment.maxLines - alignment.localLines.length).fill(
        "",
      ),
    ],
    incomingLines: [
      ...alignment.incomingLines,
      ...Array<string>(
        alignment.maxLines - alignment.incomingLines.length,
      ).fill(""),
    ],
    maxLines: alignment.maxLines,
    resultStyle: {
      height: Math.max(220, alignment.maxLines * 20 + 24),
      width: `max(100%, ${Math.max(1, ...alignment.resultLines.map((line) => Array.from(line).reduce((width, char) => width + (char.charCodeAt(0) > 255 ? 2 : 1), 0))) + 2}ch)`,
    },
  };
}

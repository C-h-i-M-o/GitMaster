import { z } from "zod";

const rpcId = z.union([
  z.number().int().nonnegative(),
  z.string().min(1).max(100),
]);
const request = z.strictObject({
  jsonrpc: z.literal("2.0"),
  id: rpcId,
  method: z.string().max(100),
  params: z.unknown(),
});
const response = z.strictObject({
  jsonrpc: z.literal("2.0"),
  id: rpcId,
  result: z.unknown().optional(),
  error: z
    .strictObject({ code: z.number(), message: z.string().max(100) })
    .optional(),
});
export type RpcFrame = z.infer<typeof request> | z.infer<typeof response>;

/** 私有 stdio 帧解码器限制单帧内存，允许跨任意字节边界接收。 */
export class FrameDecoder {
  private pending = Buffer.alloc(0);
  /** 未收齐的帧由下一块补齐，已完整的帧只解析一次。 */
  feed(chunk: Uint8Array): RpcFrame[] {
    this.pending = Buffer.concat([this.pending, chunk]);
    const frames: RpcFrame[] = [];
    let end: number;
    while ((end = this.pending.indexOf(10)) >= 0) {
      if (end > 262144) throw new Error("AI_FRAME_TOO_LARGE");
      const bytes = this.pending.subarray(0, end);
      this.pending = this.pending.subarray(end + 1);
      if (!bytes.length) continue;
      const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      frames.push(z.union([request, response]).parse(JSON.parse(text)));
    }
    if (this.pending.length > 262144) throw new Error("AI_FRAME_TOO_LARGE");
    return frames;
  }
}

/** 对写入管道的消息也施加同一上限，日志不能混入 stdout。 */
export function encodeFrame(value: unknown): string {
  const text = JSON.stringify(value);
  if (Buffer.byteLength(text) > 262144) throw new Error("AI_FRAME_TOO_LARGE");
  return `${text}\n`;
}

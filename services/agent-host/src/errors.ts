import type { AiFailure } from "./contracts.ts";

/** 从错误链提取已知原因；原始消息、响应正文与地址均不对外返回。 */
export function describeModelError(error: unknown): AiFailure {
  let current = error;
  let facts = "";
  let code = "";
  let status: number | undefined;
  for (
    let depth = 0;
    depth < 4 && current && typeof current === "object";
    depth++
  ) {
    if ("code" in current && typeof current.code === "string") {
      if (!code) code = current.code;
      facts += ` ${current.code.slice(0, 100)}`;
    }
    if ("message" in current && typeof current.message === "string")
      facts += ` ${current.message.slice(0, 4000)}`;
    if ("status" in current && typeof current.status === "number")
      status = current.status;
    current = "cause" in current ? current.cause : undefined;
  }
  const certificate = facts.match(
    /\b(SELF_SIGNED_CERT_IN_CHAIN|DEPTH_ZERO_SELF_SIGNED_CERT|UNABLE_TO_VERIFY_LEAF_SIGNATURE|UNABLE_TO_GET_ISSUER_CERT_LOCALLY|CERT_HAS_EXPIRED|CERT_NOT_YET_VALID|ERR_TLS_CERT_ALTNAME_INVALID)\b/,
  )?.[1];
  if (certificate)
    return {
      code: "AI_TLS_CERTIFICATE",
      message: `TLS 证书校验失败（${certificate}）。请检查系统证书信任、有效期和服务地址。`,
    };
  status ??= Number(
    facts.match(/\b(400|401|402|403|404|408|413|422|429|50[0-9])\b/)?.[1],
  );
  const http: Record<number, string> = {
    400: "请求格式或参数无效",
    401: "认证失败，请检查 API 密钥",
    402: "账户余额或额度不足",
    403: "没有访问权限",
    404: "接口地址或模型不存在",
    408: "服务请求超时",
    413: "请求内容超过服务限制",
    422: "请求参数不符合模型要求",
    429: "请求达到速率或并发限制",
  };
  if (status && (http[status] || (status >= 500 && status <= 599)))
    return {
      code: `AI_HTTP_${status}`,
      message: `模型服务返回 HTTP ${status}：${http[status] ?? "服务端暂时不可用"}。`,
    };
  const network = facts.match(
    /\b(ECONNREFUSED|ECONNRESET|ENOTFOUND|EAI_AGAIN|ETIMEDOUT|UND_ERR_CONNECT_TIMEOUT)\b/,
  )?.[1];
  if (network)
    return {
      code: "AI_NETWORK_ERROR",
      message: `模型服务连接失败（${network}），请检查网络、代理和服务地址。`,
    };
  const known: Record<string, string> = {
    AUTH: "模型服务认证失败，请检查 API 密钥",
    QUOTA_EXCEEDED: "模型服务账户余额或额度不足",
    RATE_LIMIT: "模型服务请求达到速率或并发限制",
    TIMEOUT: "模型请求超时",
    ABORTED: "模型请求已取消或中止",
    INVALID_REQUEST: "模型服务拒绝了请求参数",
    TRANSPORT: "模型服务网络连接失败",
    MISSING_CREDENTIAL: "未能读取模型所需的系统凭据",
    INVALID_CREDENTIAL: "模型密钥格式不符合认证头要求",
    EMPTY_RESPONSE: "模型服务返回空响应",
    STREAM_CLOSED: "模型响应流在完成之前断开",
    AI_CONNECTION_FAILED: "模型服务没有返回文本或推理内容",
  };
  return {
    code: known[code] ? code : "AI_MODEL_ERROR",
    message: known[code]
      ? `${known[code]}（${code}）。`
      : "模型请求失败，服务没有返回可识别的错误原因。",
  };
}

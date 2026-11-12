import assert from "node:assert/strict";
import { test } from "node:test";
import { describeModelError } from "./errors.ts";

test("HTTP 认证、余额和限流原因可识别，响应中的凭据和地址不外泄", () => {
  for (const status of [401, 402, 429, 503]) {
    const error = describeModelError({
      status,
      message:
        "Bearer sk-private https://secret:password@example.test/?token=private",
    });
    assert.equal(error.code, `AI_HTTP_${status}`);
    assert.match(error.message, new RegExp(String(status)));
    assert.doesNotMatch(error.message, /private|password|example|Bearer/);
  }
});
test("底层证书与 DNS 错误从 cause 中识别，未知异常不原样外传", () => {
  assert.match(
    describeModelError(
      new Error("fetch failed", { cause: { code: "CERT_HAS_EXPIRED" } }),
    ).message,
    /CERT_HAS_EXPIRED/,
  );
  assert.match(
    describeModelError({ cause: { code: "ENOTFOUND" } }).message,
    /ENOTFOUND/,
  );
  assert.doesNotMatch(
    describeModelError(new Error("sk-private")).message,
    /sk-private/,
  );
});

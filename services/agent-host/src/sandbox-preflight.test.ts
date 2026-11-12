import assert from "node:assert/strict";
import test from "node:test";
import { SandboxPreflight } from "./sandbox-preflight.ts";

/** 授权失败必须在启动命令前拒绝，同一诊断版本内不得反复初始化 ACL。 */
test("ACL 初始化失败缓存，重新检查后失效，完全访问不触发检查", async () => {
  let checks = 0;
  let ready = false;
  const gate = new SandboxPreflight(async () => {
    checks++;
    return {
      ready,
      code: "AI_SANDBOX_ACL",
      win32Code: 5,
      operation: "WRITE_OWNER",
    };
  });
  for (let i = 0; i < 2; i++)
    await assert.rejects(
      gate.check("session", "workspace-write"),
      /commandStarted=false/,
    );
  assert.equal(checks, 1);
  await gate.check("session", "danger-full-access");
  assert.equal(checks, 1);
  ready = true;
  gate.invalidate();
  await gate.check("session", "workspace-write");
  assert.equal(checks, 2);
});

/** 命令自身退出不能污染沙箱诊断，真正的初始化失败需要缓存。 */
test("健康预检复用，初始化异常覆盖缓存，普通命令错误不覆盖", async () => {
  let checks = 0;
  const gate = new SandboxPreflight(async () => {
    checks++;
    return { ready: true, code: "OK", operation: "ready", win32Code: null };
  });
  await gate.check("s", "workspace-write");
  await gate.check("s", "workspace-write");
  assert.equal(checks, 1);
  gate.rememberFailure("s", "Command exited with code 1");
  await gate.check("s", "workspace-write");
  gate.rememberFailure(
    "s",
    "SetNamedSecurityInfoW failed (win32 5): grantWrite(C:\\temporary)",
  );
  await assert.rejects(
    gate.check("s", "workspace-write"),
    /commandStarted=false/,
  );
  await assert.rejects(gate.check("s", "read-only"), /commandStarted=false/);
  assert.equal(checks, 1);
});

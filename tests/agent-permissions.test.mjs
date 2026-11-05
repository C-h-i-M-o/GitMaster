import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "vite";

test("权限说明不暗示项目外访问、任意网络或裸命令能力", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const { permissionOptions } = await server.ssrLoadModule(
      "/src/hooks/useAgentSettings.ts",
    );
    const { permissionPresentation } = await server.ssrLoadModule(
      "/src/ui/agentPresentation.ts",
    );
    const byMode = new Map(
      permissionOptions.map((option) => [option.value, option]),
    );

    assert.equal(byMode.get("requestApproval").label, "逐次审批");
    assert.match(byMode.get("requestApproval").description, /读取自动/);
    assert.match(byMode.get("requestApproval").description, /均需审批/);
    assert.doesNotMatch(byMode.get("requestApproval").description, /项目外/);
    assert.equal(byMode.get("autoApprove").label, "工作区协作");
    assert.match(byMode.get("autoApprove").description, /本地暂存和提交可自动/);
    assert.match(byMode.get("autoApprove").description, /仍需审批/);
    assert.doesNotMatch(byMode.get("autoApprove").description, /公开联网自动/);
    assert.equal(byMode.get("fullAccess").label, "自主执行");
    assert.match(byMode.get("fullAccess").description, /工作区边界/);
    assert.match(byMode.get("fullAccess").description, /沙箱命令尚未启用/);
    assert.doesNotMatch(byMode.get("fullAccess").description, /任意文件和网络/);
    assert.equal(
      permissionPresentation("requestApproval", permissionOptions).icon,
      "shield",
    );
    assert.equal(
      permissionPresentation("autoApprove", permissionOptions).icon,
      "folderCheck",
    );
    assert.equal(
      permissionPresentation("fullAccess", permissionOptions).icon,
      "shieldBolt",
    );
  } finally {
    await server.close();
  }
});

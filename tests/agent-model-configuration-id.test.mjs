import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "vite";

test("模型发现和测试请求标识只包含后端允许的十六进制字符", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule(
      "/src/ui/agentModelConfiguration.ts",
    );
    const id = helpers.configurationRequestId(1_794_000_000_000, 12345);
    assert.match(id, /^[0-9a-f-]{1,96}$/);
  } finally {
    await server.close();
  }
});

test("复用保存的 Key 时必须确认协议或来源端口改变", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule(
      "/src/ui/agentModelConfiguration.ts",
    );
    assert.equal(
      helpers.providerHostChanged(
        "https://localhost:8080/v1",
        "http://localhost:8080/v1",
      ),
      true,
    );
    assert.equal(
      helpers.providerHostChanged(
        "https://api.example.com/v1",
        "https://api.example.com/compatible/v1",
      ),
      false,
    );
    assert.equal(
      helpers.providerHostChanged(
        "https://api.example.com/v1",
        "https://api.example.com:9443/v1",
      ),
      true,
    );
  } finally {
    await server.close();
  }
});

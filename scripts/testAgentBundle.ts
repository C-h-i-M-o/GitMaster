import { existsSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = fileURLToPath(new URL("..", import.meta.url));
const bundle = join(root, "src-tauri/resources/agent");
if (!existsSync(join(bundle, "dist/main.js")))
  throw new Error("请先执行 pnpm build:agent 生成独立运行包。");
// 复用真实进程回归；仅测试进程可见此路径，不修改应用或用户环境配置。
const result = spawnSync(
  process.execPath,
  ["--test", "services/agent-host/src/host-process.test.ts"],
  {
    cwd: root,
    stdio: "inherit",
    windowsHide: true,
    env: { ...process.env, GITMASTER_AI_TEST_BUNDLE_ROOT: bundle },
  },
);
process.exit(result.status ?? 1);

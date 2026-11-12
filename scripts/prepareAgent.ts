import {
  cpSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, isAbsolute, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const target = join(root, "src-tauri/resources/agent");
const staging = join(root, "target/ai-runtime-stage");
const pnpm = process.env.npm_execpath;
if (!pnpm || !isAbsolute(pnpm) || !existsSync(pnpm))
  throw new Error("请通过 pnpm run build:agent 准备运行资源。");
if (Number(process.versions.node.split(".")[0]) !== 24)
  throw new Error("AI 运行时构建需要 Node.js 24 LTS。");

/** 清理前验证绝对路径与生成标记，不删除未经本脚本管理的目录。 */
function clearGenerated(path: string, marker: string): void {
  const absolute = resolve(path);
  if (
    !absolute.startsWith(`${root}${sep}`) ||
    ![target, staging].includes(absolute) ||
    (existsSync(absolute) && lstatSync(absolute).isSymbolicLink())
  )
    throw new Error("运行资源目录超出允许范围。");
  if (existsSync(absolute)) {
    if (!existsSync(marker))
      throw new Error("运行资源目录包含未知内容，拒绝清理。");
    rmSync(absolute, { recursive: true, force: true });
  }
}
const dependencies = createHash("sha256")
  .update(readFileSync(join(root, "pnpm-lock.yaml")))
  .update(readFileSync(join(root, "services/agent-host/package.json")))
  .update(process.version + process.platform + process.arch)
  .digest("hex");
let cached = false;
try {
  const metadata: { dependencies?: string } = JSON.parse(
    readFileSync(join(target, ".gitmaster-runtime.json"), "utf8"),
  );
  cached =
    metadata.dependencies === dependencies &&
    !lstatSync(target).isSymbolicLink() &&
    existsSync(join(target, "node_modules"));
} catch {
  /* 首次构建或元数据变化时重新部署。 */
}
if (!cached) {
  const stageMarker = join(root, "target/ai-runtime-stage.owner");
  mkdirSync(dirname(stageMarker), { recursive: true });
  clearGenerated(staging, stageMarker);
  writeFileSync(stageMarker, "GitMaster AI 生成资源\n");
  const deployment = spawnSync(
    process.execPath,
    [
      pnpm,
      "--filter",
      "@gitmaster/agent-host",
      "deploy",
      "--prod",
      "--legacy",
      "--config.node-linker=hoisted",
      staging,
    ],
    { cwd: root, stdio: "inherit", windowsHide: true },
  );
  if (deployment.status !== 0)
    throw new Error("AI 依赖部署失败，未更新桌面资源。");
}
const runtimeDirectory = cached ? target : staging;
cpSync(
  join(root, "services/agent-host/assets"),
  join(runtimeDirectory, "assets"),
  { recursive: true },
);
if (cached)
  cpSync(join(root, "services/agent-host/dist"), join(target, "dist"), {
    recursive: true,
  });
if (!existsSync(join(runtimeDirectory, "dist/main.js")))
  throw new Error("缺少 AI 运行时构建入口。");
const installedLicense = join(dirname(process.execPath), "LICENSE");
const nodeLicense = existsSync(installedLicense)
  ? installedLicense
  : join(
      root,
      `services/agent-host/licenses/node-${process.versions.node}-LICENSE`,
    );
if (!existsSync(nodeLicense))
  throw new Error(
    `缺少 Node ${process.versions.node} 对应的完整许可，请先补充构建资源。`,
  );
const executable = realpathSync(process.execPath);
copyFileSync(
  executable,
  join(runtimeDirectory, process.platform === "win32" ? "node.exe" : "node"),
);
copyFileSync(nodeLicense, join(runtimeDirectory, "NODE_LICENSE"));
copyFileSync(
  join(root, "LICENSE"),
  join(runtimeDirectory, "GITMASTER_LICENSE"),
);
copyFileSync(
  join(root, "THIRD_PARTY_NOTICES.md"),
  join(runtimeDirectory, "THIRD_PARTY_NOTICES.md"),
);
writeFileSync(
  join(runtimeDirectory, ".gitmaster-runtime.json"),
  JSON.stringify(
    {
      dependencies,
      node: process.version,
      platform: process.platform,
      arch: process.arch,
      sha256: createHash("sha256")
        .update(readFileSync(executable))
        .digest("hex"),
    },
    null,
    2,
  ),
);
if (!cached) {
  clearGenerated(target, join(target, ".gitmaster-runtime.json"));
  mkdirSync(dirname(target), { recursive: true });
  cpSync(staging, target, {
    recursive: true,
    dereference: false,
    verbatimSymlinks: true,
  });
}
const smoke = spawnSync(
  join(target, process.platform === "win32" ? "node.exe" : "node"),
  [
    "--input-type=module",
    "-e",
    "await import('@deepseek-ai/dsh-agent-loop'); await import('@deepseek-ai/dsh-sandbox-local'); console.log('AI runtime resources ready')",
  ],
  { cwd: target, stdio: "inherit", windowsHide: true },
);
if (smoke.status !== 0)
  throw new Error("内置运行资源检查失败，不能交付该桌面包。");

// deploy 的生产安装快照不应改变开发工作区下一条命令的依赖判断。
if (!cached) {
  const restore = spawnSync(
    process.execPath,
    [
      pnpm,
      "install",
      "--frozen-lockfile",
      "--prod=false",
      "--config.confirmModulesPurge=false",
    ],
    { cwd: root, stdio: "inherit", windowsHide: true },
  );
  if (restore.status !== 0)
    throw new Error(
      "运行资源已生成，但开发依赖状态恢复失败，请执行 pnpm install --frozen-lockfile。",
    );
}

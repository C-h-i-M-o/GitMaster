import { registerHooks } from "node:module";
import { realpathSync } from "node:fs";
import { isAbsolute, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const resource = process.env.GITMASTER_AI_TEST_BUNDLE_ROOT;
if (!resource) throw new Error("缺少打包运行时测试路径");
const root = realpathSync(resource);
// 测试专用：禁止 ESM 和 CommonJS 从开发仓库补齐打包遗漏的模块。
registerHooks({
  /** 保留 Node 默认解析，只检查最终文件确实来自独立资源目录。 */
  resolve(specifier, context, nextResolve) {
    const result = nextResolve(specifier, context);
    if (result.url.startsWith("file:")) {
      const path = relative(root, realpathSync(fileURLToPath(result.url)));
      if (isAbsolute(path) || path === ".." || path.startsWith(`..${sep}`))
        throw new Error("AI_BUNDLE_EXTERNAL_MODULE");
    }
    return result;
  },
});

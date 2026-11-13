/** 真实 hook 的分页恢复、请求测试、迟到结果和权限设置交互验收。 */
async (page) => {
  await page.goto(
    "http://127.0.0.1:1420/services/agent-host/fixtures/iteration-3-ui/harness.html",
  );
  await page.getByRole("button", { name: "加载成功历史", exact: true }).click();
  await page.getByText("后续轮次已经正常完成。", { exact: true }).waitFor();
  if (await page.locator(".ai-recovery").count())
    throw new Error("成功历史仍显示全局错误");
  if (!(await page.locator(".ai-error").count()))
    throw new Error("历史错误被删除");
  await page.getByRole("textbox", { name: "AI 消息" }).fill("必须保留的草稿");
  await page.getByRole("button", { name: "模拟读取失败", exact: true }).click();
  await page
    .getByRole("button", { name: "重新加载记录", exact: true })
    .waitFor();
  await page.getByRole("button", { name: "恢复读取", exact: true }).click();
  await page.getByRole("button", { name: "重新加载记录", exact: true }).click();
  await page
    .getByText("对话记录已重新加载，任务不会重新执行。", { exact: true })
    .waitFor();
  if (
    (await page.getByRole("textbox", { name: "AI 消息" }).inputValue()) !==
    "必须保留的草稿"
  )
    throw new Error("重载清掉了草稿");
  await page.getByRole("button", { name: "加载失败历史", exact: true }).click();
  await page.getByText("上一轮请求失败", { exact: true }).waitFor();
  const connect = page.getByRole("button", {
    name: "测试模型连接",
    exact: true,
  });
  await connect.click();
  if (
    !(await page
      .getByRole("button", { name: "正在测试…", exact: true })
      .isDisabled())
  )
    throw new Error("测试未禁用重复触发");
  await page.getByRole("button", { name: "连接失败", exact: true }).click();
  await page.locator('.ai-connection-result[data-state="failed"]').waitFor();
  await connect.click();
  await page.getByRole("button", { name: "完成连接测试", exact: true }).click();
  await page.locator('.ai-connection-result[data-state="succeeded"]').waitFor();
  await connect.click();
  await page.getByRole("button", { name: "加载成功历史", exact: true }).click();
  await page.getByText("后续轮次已经正常完成。", { exact: true }).waitFor();
  await page.getByRole("button", { name: "完成连接测试", exact: true }).click();
  if (await page.locator(".ai-connection-result").count())
    throw new Error("迟到测试污染新会话");
  await page.screenshot({ path: "target/iteration-3-recovery.png" });
  await page.getByRole("button", { name: "查看权限设置", exact: true }).click();
  await page
    .getByRole("heading", { name: "终端沙箱权限", exact: true })
    .waitFor();
  await page.getByRole("button", { name: "检查权限", exact: true }).click();
  await page.getByText("权限检查通过", { exact: true }).waitFor();
  if (
    (await page.locator(".settings-sandbox-path").innerText()).includes("\\?\\")
  )
    throw new Error("展示仍有扩展路径前缀");
  await page.getByRole("button", { name: "重新检查", exact: true }).click();
  await page.getByText("权限检查通过", { exact: true }).waitFor();
  await page.screenshot({ path: "target/iteration-3-settings.png" });
  return {
    historicalErrorRetained: true,
    latestSuccessClean: true,
    reloadKeepsDraft: true,
    testFailureAndRecovery: true,
    lateResultIsolated: true,
    sandboxRecheck: true,
  };
};

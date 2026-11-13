/** 审批中的会话重载后继续保留运行标识，可正常发起停止请求。 */
async (page) => {
  await page.goto(
    "http://127.0.0.1:1420/services/agent-host/fixtures/iteration-3-ui/harness.html",
  );
  await page.evaluate(() => window.fixtureAwaiting());
  await page
    .locator(".ai-approval strong")
    .filter({ hasText: "确认测试" })
    .waitFor();
  await page.evaluate(() => window.fixtureReselect());
  await page
    .getByText("对话记录已重新加载，任务不会重新执行。", { exact: true })
    .waitFor();
  await page
    .locator(".ai-approval strong")
    .filter({ hasText: "确认测试" })
    .waitFor();
  await page.getByRole("button", { name: "停止生成", exact: true }).click();
  const runId = await page.evaluate(() => window.cancelledRun);
  if (runId !== "active") throw new Error("重载后活动运行标识丢失");
  return { approvalSurvivesReload: true, activeRunStop: true };
};

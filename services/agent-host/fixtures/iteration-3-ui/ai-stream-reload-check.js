/** 同代次恢复不能丢掉后端历史未保留的已显示流式内容。 */
async (page) => {
  await page.goto(
    "http://127.0.0.1:1420/services/agent-host/fixtures/iteration-3-ui/harness.html",
  );
  await page.evaluate(() => window.fixtureAwaiting());
  await page.getByText("已经显示的流式内容", { exact: true }).waitFor();
  await page.evaluate(() => {
    window.fixtureHidePartial();
    window.fixtureReselect();
  });
  await page
    .getByText("对话记录已重新加载，任务不会重新执行。", { exact: true })
    .waitFor();
  await page
    .getByText("已经显示的流式内容", { exact: true })
    .waitFor({ timeout: 5000 });
  return { displayedStreamRetained: true };
};

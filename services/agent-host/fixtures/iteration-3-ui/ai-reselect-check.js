/** 重复选择当前侧栏项应恢复记录，而不是清空后失去订阅。 */
async (page) => {
  await page.goto(
    "http://127.0.0.1:1420/services/agent-host/fixtures/iteration-3-ui/harness.html",
  );
  await page.getByRole("button", { name: "加载成功历史", exact: true }).click();
  await page.getByText("后续轮次已经正常完成。", { exact: true }).waitFor();
  await page.evaluate(() => window.fixtureReselect());
  await page
    .getByText("后续轮次已经正常完成。", { exact: true })
    .waitFor({ timeout: 5000 });
  return { sameSessionReselectRetainsHistory: true };
};

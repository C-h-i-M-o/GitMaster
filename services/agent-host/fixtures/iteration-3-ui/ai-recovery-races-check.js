/** 验证历史分页换代、有限重建、迟到失败及协议不匹配的真实 hook 行为。 */
async (page) => {
  await page.goto(
    "http://127.0.0.1:1420/services/agent-host/fixtures/iteration-3-ui/harness.html",
  );
  await page.evaluate(() => window.fixtureEpoch(false));
  await page.getByText("后续轮次已经正常完成。", { exact: true }).waitFor();
  if (await page.locator(".ai-recovery").count())
    throw new Error("单次换代未恢复");
  const recoveredReads = await page.evaluate(() => window.fixtureEpochReads());
  if (recoveredReads !== 4)
    throw new Error(`换代读取次数异常：${recoveredReads}`);
  await page.evaluate(() => window.fixtureEpoch(true));
  await page
    .getByRole("button", { name: "重新加载记录", exact: true })
    .waitFor();
  if ((await page.evaluate(() => window.fixtureEpochReads())) !== 4)
    throw new Error("反复换代未有界停止");
  await page.evaluate(() => window.fixtureDelayed());
  await page.waitForFunction(() => window.pendingRead === true);
  await page.getByRole("button", { name: "加载成功历史", exact: true }).click();
  await page.getByText("后续轮次已经正常完成。", { exact: true }).waitFor();
  await page.evaluate(() => window.fixtureRejectRead());
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  if (await page.locator(".ai-recovery").count())
    throw new Error("上一会话的迟到失败污染当前会话");
  await page.evaluate(() => window.fixtureMissingMeta());
  await page
    .getByText("AI 运行时返回的数据与界面不匹配，请重启应用后重试。", {
      exact: true,
    })
    .waitFor();
  if (
    await page
      .getByRole("button", { name: "测试模型连接", exact: true })
      .count()
  )
    throw new Error("协议失败被误报为网络失败");
  return {
    oneEpochChangeRecovered: true,
    repeatedEpochBounded: true,
    lateReadFailureIsolated: true,
    protocolMismatchVisible: true,
  };
};

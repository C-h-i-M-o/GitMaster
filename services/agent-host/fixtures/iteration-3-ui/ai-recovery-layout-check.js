/** 恢复按钮和权限设置在两种尺寸、缩放及深浅主题下保持可操作。 */
async (page) => {
  const results = [];
  for (const sample of [
    { width: 1120, height: 760, zoom: 1, dark: false },
    { width: 860, height: 620, zoom: 1, dark: false },
    { width: 1120, height: 760, zoom: 1.25, dark: true },
  ]) {
    await page.setViewportSize({ width: sample.width, height: sample.height });
    await page.goto(
      "http://127.0.0.1:1420/services/agent-host/fixtures/iteration-3-ui/harness.html",
    );
    await page.evaluate(async (sample) => {
      const { applyAppearance } = await import("/src/ui/appearanceRuntime.ts");
      applyAppearance(
        {
          colorMode: sample.dark ? "dark" : "light",
          lightTheme: "github",
          darkTheme: "github",
          elasticity: 6,
          showLabels: true,
          aiChat: { transcriptView: "standard", collapseTiming: "completion" },
        },
        false,
      );
      document.documentElement.style.zoom = String(sample.zoom);
    }, sample);
    await page
      .getByRole("button", { name: "加载失败历史", exact: true })
      .click();
    const button = page.getByRole("button", {
      name: "测试模型连接",
      exact: true,
    });
    await button.waitFor();
    await page.keyboard.press("Tab");
    await button.focus();
    const metrics = await button.evaluate((el) => {
      const r = el.getBoundingClientRect();
      return {
        height: r.height,
        right: r.right,
        outline: getComputedStyle(el).outlineStyle,
      };
    });
    if (
      metrics.right > sample.width + 1 ||
      metrics.height < 34 * sample.zoom ||
      metrics.outline === "none"
    )
      throw new Error(JSON.stringify(metrics));
    await button.click({ trial: true });
    await page.screenshot({
      path: `target/iteration-3-recovery-layout-${sample.width}-${sample.zoom}.png`,
    });
    await page
      .getByRole("button", { name: "查看权限设置", exact: true })
      .click();
    await page.getByRole("button", { name: "检查权限", exact: true }).click();
    await page.getByText("权限检查通过", { exact: true }).waitFor();
    const settings = await page.locator(".settings-sandbox").evaluate((el) => ({
      width: el.getBoundingClientRect().width,
      scroll: el.scrollWidth,
      client: el.clientWidth,
    }));
    if (settings.scroll > settings.client + 1)
      throw new Error(JSON.stringify(settings));
    await page
      .getByRole("button", { name: "重新检查", exact: true })
      .click({ trial: true });
    await page.screenshot({
      path: `target/iteration-3-settings-layout-${sample.width}-${sample.zoom}.png`,
    });
    results.push({ ...sample, ...metrics, settings });
  }
  return results;
};

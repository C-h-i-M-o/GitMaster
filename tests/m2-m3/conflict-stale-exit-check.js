async (page) => {
 await page.goto('http://127.0.0.1:1420/tests/m2-m3/exit-harness.html');
 await page.waitForFunction(()=>window.exitHarness?.w.appSettings.editable);
 await page.evaluate(async()=>{await window.exitHarness.w.repo.open('/test');});
 await page.waitForFunction(()=>window.exitHarness.w.conflicts.conflicts!==null);
 await page.evaluate(async()=>{await window.exitHarness.w.conflicts.selectFile('c');window.exitHarness.w.conflicts.edit('失效上下文中的草稿');});
 await page.waitForFunction(()=>window.exitHarness.w.conflicts.dirty);
 await page.evaluate(()=>window.exitHarness.quit());
 await page.waitForFunction(()=>window.exitHarness.w.discardPending);
 await page.evaluate(()=>window.exitHarness.w.repo.clear());
 await page.waitForFunction(()=>window.exitHarness.w.repo.repository===null&&window.exitHarness.w.conflicts.stale);
 await page.evaluate(()=>window.exitHarness.w.discardDraft());
 const state=await page.evaluate(()=>({dirty:window.exitHarness.w.conflicts.dirty,exits:window.exitHarness.exits}));
 if(!state.dirty||state.exits!==0)throw Error(JSON.stringify(state));
 return {passed:1,state};
}

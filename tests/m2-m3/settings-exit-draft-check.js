async (page) => {
 await page.goto('http://127.0.0.1:1420/tests/m2-m3/exit-harness.html');
 await page.waitForFunction(()=>window.exitHarness?.w.appSettings.editable);
 await page.evaluate(()=>window.exitHarness.w.appSettings.edit(s=>({...s,uiPreferences:{...s.uiPreferences,showLabels:false}})));
 await page.waitForFunction(()=>window.exitHarness.w.appSettings.dirty);
 await page.evaluate(()=>window.exitHarness.quit());
 await page.waitForFunction(()=>window.exitHarness.w.settingsPending==='exit');
 await page.evaluate(()=>window.exitHarness.w.keepSettings());
 await page.waitForFunction(()=>window.exitHarness.w.settingsPending===null);
 const kept=await page.evaluate(()=>({dirty:window.exitHarness.w.appSettings.dirty,modal:window.exitHarness.w.modal,labels:window.exitHarness.w.appSettings.draft.uiPreferences.showLabels,exits:window.exitHarness.exits}));
 if(!kept.dirty||kept.modal!=='settings'||kept.labels!==false||kept.exits!==0)throw Error(JSON.stringify(kept));
 await page.evaluate(()=>window.exitHarness.w.openModal('settings')());
 await page.waitForFunction(()=>window.exitHarness.w.modal==='settings');
 const reopened=await page.evaluate(()=>({dirty:window.exitHarness.w.appSettings.dirty,labels:window.exitHarness.w.appSettings.draft.uiPreferences.showLabels}));
 if(!reopened.dirty||reopened.labels!==false)throw Error(JSON.stringify(reopened));
 return {passed:2,kept,reopened};
}

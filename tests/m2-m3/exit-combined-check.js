async () => {
 const tick=()=>new Promise(r=>setTimeout(r,50)); const h=()=>window.exitHarness;
 const check=(v,m)=>{if(!v)throw Error(m)};
 await h().w.repo.open('/test'); await tick();
 await h().w.conflicts.selectFile('c'); h().w.conflicts.edit('冲突草稿'); await tick();
 h().quit(); await tick(); check(h().w.discardPending&&h().exits===0,'冲突必须拦截');
 h().w.keepDraft(); await tick(); check(h().w.conflicts.dirty&&h().exits===0,'取消必须保留冲突');
 h().quit(); await tick(); h().w.discardDraft(); await tick(); check(!h().w.conflicts.dirty&&h().exits===1,'明确丢弃后继续');
 h().w.openDrawer('files')(); await tick(); await h().w.editor.controller.open('a.ts');
 h().w.editor.controller.edit('a.ts','文件草稿'); h().w.appSettings.edit(s=>({...s,gitPath:'/test/git2'})); await tick();
 h().quit(); await tick(); check(h().w.editorPending&&h().w.settingsPending===null&&h().exits===1,'文件应先确认');
 h().w.keepEditor(); await tick(); check(h().w.editor.dirty&&h().w.appSettings.dirty&&h().exits===1,'两份草稿取消保留');
 h().quit(); await tick(); await h().w.discardEditor(); await tick(); check(h().w.settingsPending==='exit'&&h().w.appSettings.dirty&&h().exits===1,'文件确认后仍须设置确认');
 h().w.keepSettings(); await tick(); check(h().exits===1&&h().w.appSettings.dirty,'第二步取消中止退出');
 h().quit(); await tick(); await h().w.saveSettingsAndContinue(); await tick(); check(h().exits===2&&!h().w.appSettings.dirty,'最终确认才退出');
 return {passed:8,exits:h().exits};
}

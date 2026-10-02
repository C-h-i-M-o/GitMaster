async (page) => {
 await page.goto('http://127.0.0.1:1420/tests/m2-m3/workbench-harness.html');
 await page.setViewportSize({width:1440,height:900});
 await page.getByRole('button',{name:'打开本地仓库',exact:true}).click();
 const more=page.getByRole('button',{name:'查看全部 12 个引用'});
 await more.waitFor();
 if(await more.evaluate(e=>e.parentElement.closest('[role="button"]')!==null))throw Error('引用控件被提交按钮祖先折叠');
 await more.focus();await page.keyboard.press('Enter');
 await page.getByRole('button',{name:'收起分支引用'}).waitFor();
 const commits=page.locator('.graph-node [role="button"]');
 await commits.first().focus();await page.keyboard.press('ArrowDown');
 if(!await commits.nth(1).evaluate(e=>e===document.activeElement))throw Error('方向键必须聚焦相邻提交按钮');
 const reference=page.locator('.branch-heads.expanded .branch-head').first();
 await reference.focus();await page.keyboard.press('Enter');
 if(!await reference.evaluate(e=>e===document.activeElement))throw Error('引用键盘事件不能交给提交选择');
 return {passed:3};
}

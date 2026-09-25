async (page) => {
 const fold=page.getByRole('button',{name:'展开 9994 行未更改内容',exact:true});await fold.waitFor();
 await page.locator('[data-row="9998"]').filter({hasText:'正文9998'}).waitFor();
 const initial=await page.evaluate(()=>window.diffCalls);
 if(initial.some(p=>p.startRow<9998&&p.startRow+p.count>4))throw Error('读取了折叠正文');
 await fold.click();await page.locator('[data-row="4"]').filter({hasText:'正文4'}).waitFor();
 await page.getByRole('button',{name:'收起 9994 行未更改内容',exact:true}).click();
 if(await page.locator('[data-row="4"]').count())throw Error('收起后仍挂载隐藏行');
 const dom=await page.locator('[data-row]').count();if(dom>55)throw Error('DOM 超限');
 return {initial,dom,folded:await fold.getAttribute('aria-expanded')};
}

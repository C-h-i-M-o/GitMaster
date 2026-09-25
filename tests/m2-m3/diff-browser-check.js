async (page) => {
 const viewport=page.locator('.paged-diff-viewport');
 await page.locator('[data-row="1"]').filter({hasText:'正文1'}).waitFor();
 const initial=await page.locator('[data-row]').count();
 if(initial>55)throw Error(`首屏 DOM 过多:${initial}`);
 await page.locator('[data-row="2"]').getByRole('button',{name:'下一段'}).click();
 await page.locator('[data-row="2"]').filter({hasText:'长行尾段'}).waitFor();
 await page.locator('[data-row="2"]').getByRole('button',{name:'首段'}).click();
 await page.locator('[data-row="2"]').getByRole('button',{name:'下一段'}).waitFor();
 await viewport.evaluate(e=>{e.scrollLeft=0;e.scrollTop=e.scrollHeight;});
 await page.locator('[data-row="20000"]').filter({hasText:'正文20000'}).waitFor();
 const last=await page.locator('[data-row="20000"] .diff-number').allTextContents();
 if(last.join(',')!==',10000')throw Error(`末行编号错误:${last}`);
 const final=await page.locator('[data-row]').count();if(final>55)throw Error(`末屏 DOM 过多:${final}`);
 console.log(JSON.stringify({initial,final,last,calls:await page.evaluate(()=>window.diffCalls)}));
}

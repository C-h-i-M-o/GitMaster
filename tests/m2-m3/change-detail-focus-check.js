async (page) => {
  await page.goto('http://127.0.0.1:1420/tests/m2-m3/workbench-harness.html');
  await page.setViewportSize({width:860,height:620});
  await page.getByRole('button',{name:'打开本地仓库',exact:true}).click();
  await page.getByRole('button',{name:/本地修改/}).click();
  await page.getByRole('textbox',{name:'提交说明',exact:true}).fill('保留的提交草稿');
  const grid=page.getByRole('grid');
  await grid.focus(); await page.keyboard.press('Home'); await page.keyboard.press('Enter');
  await page.getByRole('complementary',{name:'文件详细变化'}).waitFor();
  const check=(value,message)=>{if(!value)throw Error(message);};
  check(await page.getByRole('button',{name:'返回本地修改'}).evaluate(e=>document.activeElement===e),'打开后焦点必须在返回按钮');
  const narrow=await page.evaluate(()=>{
    const d=document.querySelector('.workbench-drawer'); const t=d.querySelector('textarea');t.focus();
    return {inert:d.inert,hiddenFocused:document.activeElement===t};
  });
  check(narrow.inert&&!narrow.hiddenFocused,'覆盖层下表单不能获得焦点');
  for(let i=0;i<20;i++){
    await page.keyboard.press('Tab');
    check(await page.evaluate(()=>!document.querySelector('.workbench-drawer').contains(document.activeElement)),'Tab 不能进入覆盖层下控件');
  }
  await page.getByRole('button',{name:'返回本地修改'}).focus(); await page.keyboard.press('Escape');
  check(await grid.evaluate(e=>document.activeElement===e),'Escape 必须返回原列表');
  check(await page.getByRole('textbox',{name:'提交说明',exact:true}).inputValue()==='保留的提交草稿','返回必须保留提交草稿');
  await page.setViewportSize({width:1440,height:900});
  await grid.focus();await page.keyboard.press('Enter');
  await page.waitForFunction(()=>!document.querySelector('.workbench-drawer').inert);
  await page.setViewportSize({width:860,height:620});
  await page.waitForFunction(()=>document.querySelector('.workbench-drawer').inert);
  await page.getByRole('button',{name:'返回本地修改'}).click();
  check(await grid.evaluate(e=>document.activeElement===e),'返回按钮恢复列表');
  return {passed:28,narrow};
}

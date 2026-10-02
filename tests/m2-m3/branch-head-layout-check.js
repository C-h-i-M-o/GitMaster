async (page) => {
 const check=(ok,message)=>{if(!ok)throw Error(message);};
 const sizes=()=>page.evaluate(()=>Array.from(document.querySelectorAll('.branch-head-object')).map(e=>({width:Number(e.getAttribute('width')),height:Number(e.getAttribute('height')),actualWidth:e.firstElementChild.offsetWidth,actualHeight:e.firstElementChild.offsetHeight,y:Number(e.getAttribute('y'))})));
 for(const short of [true,false]){
  await page.goto('http://127.0.0.1:1420/tests/m2-m3/workbench-harness.html'+(short?'?shortRefs':''));
  await page.setViewportSize({width:1440,height:900});
  await page.getByRole('button',{name:'打开本地仓库',exact:true}).click();
  await page.waitForFunction(()=>{const e=document.querySelector('.branch-head-object');return e&&Number(e.getAttribute('height'))===e.firstElementChild.offsetHeight;});
  let result=await sizes();check(result.every(r=>r.width===r.actualWidth&&r.height===r.actualHeight&&r.y+r.height<=-28),'实际标签尺寸必须纳入SVG区域并置于节点上方');
  if(short)check(result[0].width<310,'短引用不能保留固定310宽度');
  else{
   const more=page.getByRole('button',{name:'查看全部 12 个引用'});await more.focus();await page.keyboard.press('Enter');
   await page.waitForFunction(()=>document.querySelector('.branch-heads.expanded'));
   await page.waitForFunction(()=>{const e=document.querySelector('.branch-head-object');return Number(e.getAttribute('height'))===e.firstElementChild.offsetHeight;});
   result=await sizes();check(result.every(r=>r.width===r.actualWidth&&r.height===r.actualHeight),'展开后的实际尺寸必须同步');
   const label=page.locator('.branch-heads.expanded .branch-head').filter({hasText:'feature/中文长名称分支-1-abcdefghijklmnopqrstuvwxyz'});await label.focus();
   check((await label.getAttribute('aria-label')).includes('feature/中文长名称分支-1-abcdefghijklmnopqrstuvwxyz'),'键盘聚焦提供完整名称');
   check(await label.evaluate(e=>{const p=e.querySelector('.branch-head-name');return p.scrollWidth<=p.clientWidth+1;}),'展开名称可完整换行');
   await page.getByRole('button',{name:'收起分支引用'}).click();
  }
 }
 return {passed:6};
}

import assert from "node:assert/strict";
import test from "node:test";
import { createFileEditorController } from "../../src/hooks/fileEditorController.ts";
import type { EditableFile, RepositoryState, OperationResult } from "../../src/types/git.ts";

/** 内存文件夹具模拟真实保存前重读和操作终态，避免接触用户仓库。 */
function fixture() {
  const repository: RepositoryState = { repositoryId:"r", snapshotId:"s", rootPath:"/test", head:{kind:"unborn",name:"main"}, operations:[],changes:[] };
  let disk = "原始";
  let calls = 0;
  let pending: ((result: OperationResult) => void) | null = null;
  const document = (): EditableFile => ({fileId:"f",path:"a.ts",version:"v",text:{content:disk,bom:true,lineEnding:"none",contentVersion:disk}});
  const controller = createFileEditorController({repository:()=>repository,files:()=>({repositoryId:"r",snapshotId:"s",files:[{fileId:"f",path:"a.ts",kind:"tracked"}]}),blocked:()=>false,read:async()=>document(),save:async()=>{calls++;return new Promise<OperationResult>(resolve=>{pending=resolve});}});
  controller.setRepository("r");
  return { controller, setDisk:(text:string)=>{disk=text}, calls:()=>calls, finish:()=>{assert.ok(pending);pending({outcome:"succeeded",kind:"saveFile",operationId:"op",commitOid:null,branchName:null,clonePath:null,refresh:{status:"ready",state:repository}})} };
}

test("外部修改阻止保存，关闭和切换均保留草稿", async()=>{
 const f=fixture(); await f.controller.open("a.ts");f.controller.edit("a.ts","草稿"); f.setDisk("外部");
 assert.equal(await f.controller.save("a.ts"),false);assert.equal(f.calls(),0);assert.equal(f.controller.getSnapshot().error?.code,"FILE_CHANGED");
 f.controller.close("a.ts");assert.equal(f.controller.getSnapshot().pendingClose,"a.ts");assert.equal(f.controller.setRepository("other"),false);assert.equal(f.controller.getSnapshot().tabs[0]?.draft,"草稿");
});

test("保存并关闭期间的新输入仍然留在打开的脏文档",async()=>{
 const f=fixture();await f.controller.open("a.ts");f.controller.edit("a.ts","提交文本");f.controller.close("a.ts");
 const saving=f.controller.saveAndClose();await Promise.resolve();assert.equal(f.calls(),1);f.controller.edit("a.ts","更新输入");f.finish();await saving;
 assert.equal(f.controller.getSnapshot().tabs.length,1);assert.equal(f.controller.getSnapshot().tabs[0]?.baseline,"提交文本");assert.equal(f.controller.getSnapshot().tabs[0]?.draft,"更新输入");assert.equal(f.controller.dirty(),true);
});

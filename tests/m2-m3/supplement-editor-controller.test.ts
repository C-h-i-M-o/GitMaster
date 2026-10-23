import assert from "node:assert/strict";
import test from "node:test";
import { createFileEditorController } from "../../src/hooks/fileEditorController.ts";
import type { EditableFile, RepositoryState } from "../../src/types/git.ts";
import { createRepositoryController } from "../../src/hooks/repositoryController.ts";

/** 生成可手动完成的读取，稳定复现用户操作与后台重读交错。 */
function delayedRead<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

test("切项目期间冻结旧草稿，失败释放门禁且普通刷新仍可编辑", async () => {
  const original: RepositoryState = { repositoryId: "a", snapshotId: "s", rootPath: "/a", head: { kind: "unborn", name: "main" }, operations: [], changes: [] };
  let next = delayedRead<RepositoryState>();
  const repo = createRepositoryController({ openRepository: () => next.promise, readRepositoryState: () => next.promise, readFileDiff: async () => ({ kind: "binary" }), closeFileDiff: async () => {} });
  const opening = repo.open("/a"); next.resolve(original); await opening;
  const editor = createFileEditorController({
    repository: () => repo.getSnapshot().repository,
    files: () => ({ repositoryId: "a", snapshotId: "s", files: [{ fileId: "f", path: "a", kind: "tracked" }] }),
    blocked: () => repo.getSnapshot().loading && repo.getSnapshot().openingPath !== null,
    read: async () => ({ fileId: "f", path: "a", version: "v", text: { content: "旧文本", bom: false, lineEnding: "none", contentVersion: "v" } }),
    save: async (_r, d, content) => ({ ...d, text: { ...d.text, content } }),
  });
  editor.setRepository("a"); await editor.open("a");
  next = delayedRead(); const switching = repo.open("/b");
  editor.edit("a", "切换时不应接受"); assert.equal(editor.dirty(), false);
  next.resolve({ ...original, repositoryId: "b", rootPath: "/b" }); await switching;
  editor.edit("a", "切换返回与 effect 安装之间也不能输入"); assert.equal(editor.dirty(), false);
  assert.equal(editor.setRepository("b"), true);
  next = delayedRead(); const returning = repo.open("/a"); next.resolve(original); await returning;
  editor.setRepository("a"); await editor.open("a");
  next = delayedRead(); const failing = repo.open("/failed");
  next.reject({ code: "ACCESS_DENIED" }); await failing;
  editor.edit("a", "打开失败后能继续编辑"); assert.equal(editor.dirty(), true);
  assert.equal(await editor.save("a"), true);
  next = delayedRead(); const refresh = repo.refresh();
  editor.edit("a", "刷新仍能编辑"); assert.equal(editor.dirty(), true);
  next.resolve(original); await refresh; assert.equal(await editor.save("a"), true);
});

test("后台逐份重读不取消用户新开的 C 文档", async () => {
  const repository: RepositoryState = { repositoryId: "r", snapshotId: "s", rootPath: "/test", head: { kind: "unborn", name: "main" }, operations: [], changes: [] };
  const reads: string[] = [];
  const pending = new Map<string, ReturnType<typeof delayedRead<EditableFile>>>();
  let delayed = false;
  /** 为路径生成带版本的完整正文。 */
  const document = (path: string): EditableFile => ({ fileId: path, path, version: "v", text: { content: path, bom: false, lineEnding: "none", contentVersion: "v" } });
  const editor = createFileEditorController({
    repository: () => repository,
    files: () => ({ repositoryId: "r", snapshotId: "s", files: ["a", "b", "c"].map(path => ({ fileId: path, path, kind: "tracked" as const })) }),
    blocked: () => false,
    read: async (_r, path, signal) => {
      reads.push(path); if (!delayed) return document(path);
      const task = delayedRead<EditableFile>(); pending.set(path, task);
      // 故意让取消响应延后返回，验证代次检查而非只依赖 AbortSignal。
      return task.promise;
    },
    save: async (_r, d, content) => ({ ...d, text: { ...d.text, content } }),
  });
  editor.setRepository("r"); await editor.open("a"); await editor.open("b"); delayed = true;
  const background = editor.reloadCleanDocuments();
  const opening = editor.open("c"); pending.get("a")!.resolve(document("a"));
  assert.equal(await background, false);
  assert.deepEqual(reads, ["a", "b", "a", "c"]);
  pending.get("c")!.resolve(document("c")); await opening;
  assert.equal(editor.getSnapshot().activePath, "c");
  assert.equal(editor.getSnapshot().tabs.length, 3);
  assert.equal(editor.getSnapshot().error, null);
});

test("保存取消过时的文档重读，迟到正文不能覆盖已保存结果", async () => {
  const repository: RepositoryState = { repositoryId: "r", snapshotId: "s", rootPath: "/test", head: { kind: "unborn", name: "main" }, operations: [], changes: [] };
  const document: EditableFile = { fileId: "f", path: "a", version: "v", text: { content: "old", bom: false, lineEnding: "none", contentVersion: "v" } };
  let reads = 0;
  let signal: AbortSignal | undefined;
  let complete: (value: EditableFile) => void = () => { throw new Error("读取尚未开始"); };
  const controller = createFileEditorController({
    repository: () => repository,
    files: () => ({ repositoryId: "r", snapshotId: "s", files: [{ fileId: "f", path: "a", kind: "tracked" }] }),
    blocked: () => false,
    read: async (_repository, _fileId, cancellation) => {
      if (++reads === 1) return document;
      signal = cancellation;
      return new Promise<EditableFile>((resolve) => { complete = resolve; });
    },
    save: async (_repository, opened, content) => {
      assert.equal(signal?.aborted, true);
      assert.equal(opened.version, "v");
      return { ...opened, version: "saved", text: { ...opened.text, content } };
    },
  });
  controller.setRepository("r");
  await controller.open("a");
  const reload = controller.reload("a");
  controller.edit("a", "saved content");
  assert.equal(await controller.save("a"), true);
  complete(document);
  assert.equal(await reload, false);
  assert.equal(controller.getSnapshot().tabs[0]?.draft, "saved content");
  assert.equal(controller.getSnapshot().tabs[0]?.document.version, "saved");
  assert.equal(controller.getSnapshot().error, null);
});

test("目录列表失效仍使用打开文档的保存能力，失败不重试也不覆盖草稿", async()=>{
 const repository:RepositoryState={repositoryId:'r',snapshotId:'s',rootPath:'/test',head:{kind:'unborn',name:'main'},operations:[],changes:[]};let listed=true;let attempts=0;let failure='';
 const c=createFileEditorController({repository:()=>repository,files:()=>listed?{repositoryId:'r',snapshotId:'f',files:[{fileId:'opened',path:'a',kind:'tracked'}]}:null,blocked:()=>false,read:async()=>({fileId:'opened',path:'a',version:'v',text:{content:'before',bom:false,lineEnding:'none',contentVersion:'v'}}),save:async(_r,doc,content)=>{attempts++;assert.equal(doc.fileId,'opened');if(failure)throw {code:failure};return {...doc,text:{...doc.text,content}};}});
 c.setRepository('r');await c.open('a');listed=false;c.edit('a','first');assert.equal(await c.save('a'),true);
 for(const code of ['FILE_CHANGED','WRITE_OUTCOME_UNKNOWN','STALE_REQUEST']){const before=attempts;failure=code;c.edit('a','draft');assert.equal(await c.save('a'),false);assert.equal(attempts,before+1);assert.equal(c.getSnapshot().tabs[0]?.draft,'draft');}
});

test("文件ID重签不换掉原始保存版本，落盘返回的新版本用于后续保存", async () => {
 const repository:RepositoryState={repositoryId:'r',snapshotId:'s',rootPath:'/test',head:{kind:'unborn',name:'main'},operations:[],changes:[]};
 let fileId='old';let reads=0;const versions:string[]=[];
 const original:EditableFile={fileId,path:'a',version:'opened-version',text:{content:'before',bom:false,lineEnding:'none',contentVersion:'v'}};
 const controller=createFileEditorController({repository:()=>repository,files:()=>({repositoryId:'r',snapshotId:'f',files:[{fileId,path:'a',kind:'tracked'}]}),blocked:()=>false,read:async()=>{reads++;return original;},save:async(_repo,doc,content)=>{assert.equal(doc.fileId,'old');versions.push(doc.version);return {...doc,version:'saved-version',text:{...doc.text,content}};}});
 controller.setRepository('r');await controller.open('a');fileId='new';controller.edit('a','first');assert.equal(await controller.save('a'),true);
 controller.edit('a','second');assert.equal(await controller.save('a'),true);assert.equal(reads,1);assert.deepEqual(versions,['opened-version','saved-version']);
});

/** 内存文件夹具模拟真实保存前重读和操作终态，避免接触用户仓库。 */
function fixture() {
  const repository: RepositoryState = { repositoryId:"r", snapshotId:"s", rootPath:"/test", head:{kind:"unborn",name:"main"}, operations:[],changes:[] };
  let disk = "原始";
  let calls = 0;
  let pending: ((result: EditableFile) => void) | null = null;
  let submitted = "";
  const document = (): EditableFile => ({fileId:"f",path:"a.ts",version:"v",text:{content:disk,bom:true,lineEnding:"none",contentVersion:disk}});
  const controller = createFileEditorController({repository:()=>repository,files:()=>({repositoryId:"r",snapshotId:"s",files:[{fileId:"f",path:"a.ts",kind:"tracked"}]}),blocked:()=>false,read:async()=>document(),save:async(_r,d,content)=>{calls++; if (d.version !== "v" || disk !== "原始") throw { code: "FILE_CHANGED" }; submitted = content; return new Promise<EditableFile>(resolve=>{pending=resolve;});}});
  controller.setRepository("r");
  return { controller, setDisk:(text:string)=>{disk=text}, calls:()=>calls, finish:()=>{assert.ok(pending);disk=submitted;pending(document())} };
}

test("保存结果不匹配时关闭和切换均保留草稿", async()=>{
 const f=fixture(); await f.controller.open("a.ts");f.controller.edit("a.ts","草稿"); f.setDisk("外部");
 assert.equal(await f.controller.save("a.ts"),false);assert.equal(f.calls(),1);assert.equal(f.controller.getSnapshot().error?.code,"FILE_CHANGED");
 f.controller.close("a.ts");assert.equal(f.controller.getSnapshot().pendingClose,"a.ts");assert.equal(f.controller.setRepository("other"),false);assert.equal(f.controller.getSnapshot().tabs[0]?.draft,"草稿");
});

test("保存并关闭期间的新输入仍然留在打开的脏文档",async()=>{
 const f=fixture();await f.controller.open("a.ts");f.controller.edit("a.ts","提交文本");f.controller.close("a.ts");
 const saving=f.controller.saveAndClose();await Promise.resolve();assert.equal(f.calls(),1);f.controller.edit("a.ts","更新输入");f.finish();await saving;
 assert.equal(f.controller.getSnapshot().tabs.length,1);assert.equal(f.controller.getSnapshot().tabs[0]?.baseline,"提交文本");assert.equal(f.controller.getSnapshot().tabs[0]?.draft,"更新输入");assert.equal(f.controller.dirty(),true);
});

test("文件会话快照独立于仓库 Git 快照且相同文件在途只读取一次", async () => {
 const repository: RepositoryState = { repositoryId:"r", snapshotId:"git-2", rootPath:"/test", head:{kind:"unborn",name:"main"}, operations:[],changes:[] };
 let resolve: ((value: EditableFile) => void) | null = null;
 let reads = 0;
 const controller = createFileEditorController({
  repository:()=>repository,
  files:()=>({repositoryId:"r",snapshotId:"files-1",files:[{fileId:"f",path:"a.ts",kind:"tracked"}]}),
  blocked:()=>false,
  read:async()=>{ reads++; return new Promise<EditableFile>(r=>{resolve=r;}); },
  save:async(_r,d,content)=>({...d,text:{...d.text,content,contentVersion:content}}),
 });
 controller.setRepository("r");
 const first = controller.open("a.ts");
 const second = controller.open("a.ts");
 await Promise.resolve();
 assert.equal(reads, 1);
 resolve?.({fileId:"f",path:"a.ts",version:"v",text:{content:"正文",bom:false,lineEnding:"lf",contentVersion:"v"}});
 await Promise.all([first,second]);
 assert.equal(controller.getSnapshot().tabs.length, 1);
});

test("文件落盘成功而状态刷新失败时确认保存事实，并覆盖完整保存事务门禁", async () => {
 const repository: RepositoryState = { repositoryId:"r", snapshotId:"s", rootPath:"/test", head:{kind:"unborn",name:"main"}, operations:[], changes:[] };
 const changes: boolean[] = [];
 let readingSave = false;
 const controller = createFileEditorController({
  repository:()=>repository, files:()=>({repositoryId:"r",snapshotId:"f",files:[{fileId:"file",path:"a",kind:"tracked"}]}), blocked:()=>false,
  onSavingChange:(active)=>changes.push(active),
  read:async()=>{ if(readingSave) assert.equal(changes.at(-1),true); return {fileId:"file",path:"a",version:"v",text:{content:"old",bom:false,lineEnding:"none",contentVersion:"v"}}; },
  save:async(_r,d,content)=>({...d,text:{...d.text,content,contentVersion:content}}),
 });
 controller.setRepository("r"); await controller.open("a"); controller.edit("a","new"); readingSave=true;
 assert.equal(await controller.save("a"),true);
 assert.equal(controller.dirty(),false);
 assert.match(controller.getSnapshot().notice ?? "",/文件已保存/);
 assert.deepEqual(changes,[true,false]);
});

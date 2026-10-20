import assert from "node:assert/strict";
import test from "node:test";
import { mergeTreeFiles } from "../../src/ui/projectTree.ts";
import type { ProjectTreePage } from "../../src/types/git.ts";
import { workspaceDocumentPaths } from "../../src/ui/workspacePath.ts";

test("分页合并只安装文件能力，同路径刷新身份，跨仓库丢弃旧能力",()=>{
 const page:ProjectTreePage={repositoryId:"r",snapshotId:"s",treeId:"tree",directoryId:"root",entries:[{kind:"directory",id:"d",path:"src",name:"src"},{kind:"file",id:"a",path:"a",name:"a",status:"tracked"}],nextOffset:null,total:2};
 const first=mergeTreeFiles(null,page);assert.deepEqual(first.files,[{fileId:"a",path:"a",kind:"tracked"}]);
 const next=mergeTreeFiles(first,{...page,entries:[{kind:"file",id:"b",path:"src/b",name:"b",status:"untracked"}]});assert.equal(next.files.length,2);
 const refreshed=mergeTreeFiles(next,{...page,entries:[{kind:"file",id:"new-a",path:"a",name:"a",status:"tracked"}]});assert.equal(refreshed.files.find(file=>file.path==="a")?.fileId,"new-a");
 assert.equal(mergeTreeFiles(refreshed,{...page,repositoryId:"other",entries:[]}).files.length,0);
});

test("超过 50 个文档仍保留活动文档，且不修改原列表", () => {
  const paths = Array.from({ length: 51 }, (_, index) => `${index}.md`);
  const original = [...paths];
  const stored = workspaceDocumentPaths(paths, "50.md");
  assert.equal(stored.length, 50);
  assert.equal(stored.at(-1), "50.md");
  assert.deepEqual(stored.slice(0, 49), paths.slice(0, 49));
  assert.deepEqual(paths, original);
});

test("未超限与未知活动路径不补入无效文档", () => {
  assert.deepEqual(workspaceDocumentPaths(["a", "b"], "b"), ["a", "b"]);
  assert.deepEqual(workspaceDocumentPaths(["a"], "missing"), ["a"]);
  assert.deepEqual(workspaceDocumentPaths([], "missing"), []);
  assert.equal(workspaceDocumentPaths(Array.from({ length: 60 }, (_, i) => `${i}`), null).length, 50);
});

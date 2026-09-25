import assert from "node:assert/strict";
import test from "node:test";
import { mergeTreeFiles } from "../../src/ui/projectTree.ts";
import type { ProjectTreePage } from "../../src/types/git.ts";

test("分页合并只安装文件能力，同路径刷新身份，跨仓库丢弃旧能力",()=>{
 const page:ProjectTreePage={repositoryId:"r",snapshotId:"s",treeId:"tree",directoryId:"root",entries:[{kind:"directory",id:"d",path:"src",name:"src"},{kind:"file",id:"a",path:"a",name:"a",status:"tracked"}],nextOffset:null,total:2};
 const first=mergeTreeFiles(null,page);assert.deepEqual(first.files,[{fileId:"a",path:"a",kind:"tracked"}]);
 const next=mergeTreeFiles(first,{...page,entries:[{kind:"file",id:"b",path:"src/b",name:"b",status:"untracked"}]});assert.equal(next.files.length,2);
 const refreshed=mergeTreeFiles(next,{...page,entries:[{kind:"file",id:"new-a",path:"a",name:"a",status:"tracked"}]});assert.equal(refreshed.files.find(file=>file.path==="a")?.fileId,"new-a");
 assert.equal(mergeTreeFiles(refreshed,{...page,repositoryId:"other",entries:[]}).files.length,0);
});

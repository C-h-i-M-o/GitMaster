import assert from "node:assert/strict";
import test from "node:test";
import { flattenFileTree } from "../../src/ui/virtualTree.ts";
import type { FileTreeNode } from "../../src/ui/fileTree.ts";

test("虚拟树保留层级、父路径和稳定身份，分页位于所属目录后",()=>{
 const nodes:FileTreeNode[]=[{kind:"directory",name:"src",path:"src",children:[{kind:"file",name:"a",path:"src/a",fileId:"id",status:"tracked"}]}];
 const rows=flattenFileTree(nodes,()=>true,()=>true,()=>false);
 assert.deepEqual(rows.map(row=>row.key),["directory:src","file:src/a","page:src","page:"]);
 assert.deepEqual(rows.map(row=>row.depth),[1,2,2,1]);
 assert.equal(rows[1]?.parent,"src");
 assert.equal(flattenFileTree(nodes,()=>false,()=>false,()=>false).length,1);
});

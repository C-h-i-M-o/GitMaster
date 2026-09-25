import assert from "node:assert/strict";
import test from "node:test";
import { flattenChanges } from "../../src/ui/virtualChanges.ts";
import type { FileChange } from "../../src/types/git.ts";

test("MM 同文件的两侧保留独立 key 和暂存方向",()=>{
 const file:FileChange={changeId:"same",path:"a.ts",originalPath:null,indexStatus:"M",worktreeStatus:"M",kind:"tracked",binary:"text"};
 const rows=flattenChanges({staged:[file],unstaged:[file],untracked:[]});
 const files=rows.filter(row=>row.kind==="file");
 assert.equal(files.length,2);assert.notEqual(files[0]?.key,files[1]?.key);
 assert.deepEqual(files.map(row=>[row.side,row.action]),[["unstaged","stage"],["staged","unstage"]]);
 assert.equal(rows.filter(row=>row.kind==="empty").length,1);
});

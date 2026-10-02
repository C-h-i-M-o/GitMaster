import {test} from 'node:test';
import assert from 'node:assert/strict';
import {branchHeads} from '../../src/ui/branchHeadPresentation.ts';
import {selectionAction} from '../../src/ui/selectionAction.ts';
import {parseUnifiedDiff} from '../../src/ui/diffPresentation.ts';
import type {HistoryPage} from '../../src/types/git.ts';

const oid='a'.repeat(40);
const refs:HistoryPage['tips']=[
 {refId:'r',name:'origin/main',kind:'remote',oid},
 {refId:'l',name:'topic/中文很长的分支名称',kind:'local',oid},
 {refId:'m',name:'main',kind:'local',oid},
];
test('真实 HEAD 优先、名称完整且不修改原引用数组',()=>{
 const before=structuredClone(refs);
 const heads=branchHeads(refs,{kind:'branch',name:'main',oid},oid);
 assert.deepEqual(heads.map(h=>h.id),['m','l','r']);
 assert.equal(heads[0]?.current,true);assert.equal(heads[1]?.name,refs[1]?.name);
 assert.deepEqual(refs,before);
});
test('分离 HEAD 独立展示，其他 tip 不误标当前',()=>{
 const heads=branchHeads(refs,{kind:'detached',oid},oid);
 assert.equal(heads[0]?.kind,'detached');assert.equal(heads.filter(h=>h.current).length,1);
 assert.equal(branchHeads(refs,{kind:'detached',oid:'b'.repeat(40)},oid).some(h=>h.current),false);
});
test('MM 两侧相同身份仍产生混选阻断，各单侧唯一动作',()=>{
 assert.equal(selectionAction([],[]).kind,'none');
 assert.equal(selectionAction(['same'],[]).kind,'stage');
 assert.equal(selectionAction([],['same']).kind,'unstage');
 const mixed=selectionAction(['same'],['same']);assert.equal(mixed.kind,'mixed');assert.match(mixed.reason,/不能同时/);
});
test('重命名 patch 多 hunk 与无尾换行保留各侧行号',()=>{
 const patch='diff --git a/旧.txt b/新.txt\nrename from 旧.txt\nrename to 新.txt\n@@ -2,2 +2,2 @@\n 原样\n-old\n+new\n\\ No newline at end of file\n@@ -10 +20 @@\n-x\n+y\n';
 const result=parseUnifiedDiff(patch,false);
 assert.equal(result.additions,2);assert.equal(result.deletions,2);
 assert.deepEqual(result.lines.filter(l=>l.kind==='add').map(l=>l.newLine),[3,20]);
 assert.deepEqual(result.lines.filter(l=>l.kind==='delete').map(l=>l.oldLine),[3,10]);
 assert.equal(result.lines.find(l=>l.kind==='note')?.oldLine,null);
});
test('删除为空文件侧，未跟踪的 patch 样文字仍是原文',()=>{
 const deleted=parseUnifiedDiff('@@ -1,2 +0,0 @@\n-a\n-b\n',false);
 assert.equal(deleted.additions,0);assert.equal(deleted.deletions,2);
 assert.deepEqual(deleted.lines.slice(1).map(l=>l.newLine),[null,null]);
 const raw=parseUnifiedDiff('@@ -1 +1 @@\n+literal\n',true);
 assert.equal(raw.additions,2);assert.equal(raw.lines[0]?.text,'@@ -1 +1 @@');
 assert.equal(raw.lines[1]?.text,'+literal');
 assert.deepEqual(parseUnifiedDiff('',true).lines,[]);
});

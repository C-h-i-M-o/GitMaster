import assert from 'node:assert/strict';
import test from 'node:test';
import {diffLayout} from '../../src/ui/pagedDiffLayout.ts';
test('折叠保留上下文两端，展开恢复原行号及按钮',()=>{const folded=diffLayout(24,[{startRow:1,count:20}],new Set());assert.deepEqual(folded.filter(x=>x.kind==='line').map(x=>x.source),[0,1,2,3,18,19,20,21,22,23]);assert.deepEqual(folded[4],{kind:'fold',source:1,count:14,expanded:false});const opened=diffLayout(24,[{startRow:1,count:20}],new Set([1]));assert.equal(opened.length,25);assert.deepEqual(opened.filter(x=>x.kind==='line').map(x=>x.source),Array.from({length:24},(_,i)=>i));});

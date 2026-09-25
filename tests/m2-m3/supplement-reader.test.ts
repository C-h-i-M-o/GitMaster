import assert from 'node:assert/strict';
import test from 'node:test';
import { createPagedReader } from '../../src/hooks/pagedReaderController.ts';
import type { ReadDocument, ReadPage } from '../../src/types/git.ts';
const document: ReadDocument = {documentId:'d',fileId:'f',path:'large.txt',byteLength:10000000,encoding:'utf8',lineCount:100001,bom:false,lineEnding:'lf'};
/** 短页模拟后端字节上限，验证可见区不会假定请求行数等于响应行数。 */
function fixture() {
 const closed:string[]=[]; const requested:number[]=[];
 const reader=createPagedReader({open:async()=>document,close:async(id)=>{closed.push(id)},read:async(id,start,count,offset)=>{requested.push(start);return {documentId:id,startLine:start,lines:Array.from({length:Math.min(count,3)},(_,i)=>({lineNumber:start+i+1,text:`行${start+i+1}`,byteOffset:offset,nextByteOffset:null})),nextLine:start+Math.min(count,3)}},search:async()=>({documentId:'d',lines:[99999],nextLine:99999})});
 return {reader,closed,requested};
}
test('短页补齐、缓存有界、跳读不扫描前面正文',async()=>{
 const f=fixture();await f.reader.open();await f.reader.ensure(0,15);assert.equal(f.reader.line(14)?.text,'行15');assert.deepEqual(f.requested,[0,3,6,9,12]);
 for(let start=100;start<1000;start+=100) await f.reader.ensure(start,start+3);
 assert.ok(f.reader.getSnapshot().pages.length<=8);assert.equal(f.reader.line(0),undefined);assert.equal(f.reader.line(900)?.text,'行901');f.reader.dispose();assert.deepEqual(f.closed,['d']);
});
test('关闭期间迟到的打开必须释放，不能发布文档',async()=>{
 let finish:(value:ReadDocument)=>void=()=>{};const closed:string[]=[];
 const reader=createPagedReader({open:()=>new Promise(resolve=>{finish=resolve}),close:async(id)=>{closed.push(id)},read:async()=>{throw Error('不应读取')},search:async()=>{throw Error('不应查找')}});
 const pending=reader.open();reader.dispose();finish(document);await pending;assert.equal(reader.getSnapshot().document,null);assert.deepEqual(closed,['d']);
});
test('旧页不能进入重开的文档，文件变化停止自动请求',async()=>{
 let finish:(value:ReadPage)=>void=()=>{};let calls=0;
 const reader=createPagedReader({open:async()=>document,close:async()=>{},read:async()=>{calls++;if(calls===1)return new Promise(resolve=>{finish=resolve});throw {code:'FILE_CHANGED'}},search:async()=>({documentId:'d',lines:[],nextLine:null})});
 await reader.open();const loading=reader.ensure(0,1);await Promise.resolve();reader.dispose();finish({documentId:'d',startLine:0,lines:[{lineNumber:1,text:'旧',byteOffset:0,nextByteOffset:null}],nextLine:null});await loading;assert.equal(reader.line(0),undefined);
 await reader.open();await reader.ensure(0,1);await reader.ensure(1,2);assert.equal(calls,2);assert.equal(reader.getSnapshot().error?.code,'FILE_CHANGED');reader.dispose();
});
test('长行续读替换当前段，滚动补页等待续读结束',async()=>{
 let finish:(value:ReadPage)=>void=()=>{};const calls:number[]=[];
 const reader=createPagedReader({open:async()=>document,close:async()=>{},search:async()=>({documentId:'d',lines:[],nextLine:null}),read:async(id,start,count,offset)=>{calls.push(offset);if(offset)return new Promise(resolve=>{finish=resolve});return {documentId:id,startLine:start,lines:Array.from({length:count},(_,i)=>({lineNumber:start+i+1,text:'首段',byteOffset:0,nextByteOffset:16384})),nextLine:start+count}}});
 await reader.open();await reader.ensure(0,2);const next=reader.segment(0,16384);const loading=reader.ensure(5,7);assert.deepEqual(calls,[0,16384]);finish({documentId:'d',startLine:0,lines:[{lineNumber:1,text:'尾段',byteOffset:16384,nextByteOffset:null}],nextLine:1});await next;await loading;assert.equal(reader.line(0)?.text,'尾段');assert.equal(reader.line(6)?.text,'首段');reader.dispose();
});

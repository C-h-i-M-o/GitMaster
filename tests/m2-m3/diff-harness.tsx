import {createRoot} from 'react-dom/client';
import {mockIPC} from '@tauri-apps/api/mocks';
import {PagedDiff} from '../../src/components/PagedDiff';
import '../../src/styles.css';
const folded=new URLSearchParams(location.search).has('fold');
const calls:unknown[]=[];Object.assign(window,{diffCalls:calls});
mockIPC((cmd,payload)=>{const p=payload as Record<string,unknown>;calls.push({cmd,...p});if(cmd!=='read_diff_page')throw Error(cmd);const start=Number(p.startRow),offset=Number(p.byteOffset),count=Math.min(Number(p.count),16,20001-start);return {startRow:start,rows:Array.from({length:count},(_,i)=>{const n=start+i;return {kind:n===0?'hunk':n<=10000?(folded?'context':'delete'):'add',oldLine:n>0&&n<=10000?n:null,newLine:n>10000?n-10000:null,text:n===0?'@@ -1,10000 +1,10000 @@':n===2?(offset?'长行尾段':'长'.repeat(5400)):`正文${n}`,byteOffset:offset,nextByteOffset:n===2&&!offset?16200:null}}),nextRow:start+count<20001?start+count:null};});
/** 实际分页组件通过官方 IPC 替身获取短页。 */
function Harness(){return <div style={{width:850}}><PagedDiff scope={{repositoryId:'r',snapshotId:'s'}} document={{kind:'paged',folds:folded?[{startRow:1,count:10000}]:[],documentId:'d',rowCount:20001,hunkCount:1,additions:10000,deletions:10000,truncated:false}}/></div>}
createRoot(document.getElementById('root')!).render(<Harness/>);

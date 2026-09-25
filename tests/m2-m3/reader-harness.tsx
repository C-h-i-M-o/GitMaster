import { createRoot } from 'react-dom/client';
import { mockIPC } from '@tauri-apps/api/mocks';
import { PagedReader } from '../../src/components/PagedReader';
import '../../src/styles.css';
const calls:unknown[]=[];
Object.assign(window,{readerCalls:calls});
mockIPC((cmd,payload)=>{
 const p=payload as Record<string,unknown>;calls.push({cmd,...p});
 if(cmd==='open_read_document')return {documentId:'d',fileId:'f',path:'large.txt',byteLength:10485760,encoding:'utf8',lineCount:100001,bom:true,lineEnding:'mixed'};
 if(cmd==='close_read_document')return;
 if(cmd==='search_read_document')return {documentId:'d',lines:p.query==='目标'&&Number(p.startLine)<90000?[90000]:[],nextLine:null};
 if(cmd==='read_document_page') {
  const start=Number(p.startLine),offset=Number(p.byteOffset),count=Math.min(Number(p.count),16,100001-start);
  return {documentId:'d',startLine:start,lines:Array.from({length:count},(_,i)=>({lineNumber:start+i+1,text:start+i===2?(offset?'长行尾段':'长'.repeat(5400)):`正文第${start+i+1}行`,byteOffset:offset,nextByteOffset:start+i===2&&!offset?16200:null})),nextLine:start+count<100001?start+count:null};
 }
 throw Error(cmd);
});
/** 使用实际生产组件与官方 IPC 替身验证十万行视口。 */
function Harness(){return <div style={{height:500,width:900,display:'flex'}}><PagedReader scope={{repositoryId:'r',snapshotId:'s'}} fileId="f"/></div>}
createRoot(document.getElementById('root')!).render(<Harness/>);

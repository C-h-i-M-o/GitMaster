import {createRoot} from 'react-dom/client';
import {mockIPC} from '@tauri-apps/api/mocks';
import {useWorkbench} from '../../src/hooks/useWorkbench';
import {defaultSettings} from '../../src/ui/settingsDraft';
let exits=0, fail=false;
const repository={repositoryId:'r',snapshotId:'s',rootPath:'/test',head:{kind:'branch',name:'main',oid:'a'.repeat(40)},operations:['merge'],changes:[]};
Object.assign(window,{isTauri:true});
mockIPC((cmd,payload)=>{
 if(cmd==='read_operation')return null;
 if(cmd==='read_app_settings')return {revision:'1',settings:defaultSettings()};
 if(cmd==='apply_app_settings'){if(fail)throw {code:'SETTINGS_IO',fieldErrors:[]};return {revision:'2',settings:(payload as {draft:unknown}).draft};}
 if(cmd==='detect_git')return {status:'ready',executablePath:'/test/git',version:'git version 2.49.0',source:'path'};
 if(cmd==='open_repository'||cmd==='read_repository_state')return repository;
 if(cmd==='read_repository_watch')return {repositoryId:'r',revision:0,reliable:true};
 if(cmd==='read_project_tree')return {repositoryId:'r',snapshotId:'s',treeId:'t',directoryId:null,entries:[{id:'f',kind:'file',path:'a.ts',name:'a.ts',status:'tracked'}],nextOffset:null,truncated:false};
 if(cmd==='read_editable_file')return {fileId:'f',path:'a.ts',version:'v',text:{content:'原始',bom:false,lineEnding:'none',contentVersion:'原始'}};
 if(cmd==='read_conflicts')return {repositoryId:'r',mergeSessionId:'m',headOid:'a'.repeat(40),mergeHeadOids:['b'.repeat(40)],files:[{conflictId:'c',path:'conflict.ts',stageOids:{base:null,local:null,incoming:null},editorSupport:{status:'supported'}}]};
 if(cmd==='read_conflict_document')return {mergeSessionId:'m',conflictId:'c',base:'base',local:'local',incoming:'incoming',result:'原始',encoding:'utf8',lineEnding:'lf',fingerprint:'v'};
 if(cmd==='plugin:event|listen')return 1;
 if(cmd==='plugin:event|unlisten')return null;
 throw {code:'TEST_UNSUPPORTED',message:cmd,retryable:false};
});
/** 使用实际工作台 hook 验证退出延续不绕过设置草稿。 */
function Harness(){const w=useWorkbench();Object.assign(window,{exitHarness:{w,get exits(){return exits;},setFail:(value:boolean)=>{fail=value;},quit:()=>w.guardExit(async()=>{exits++;})}});return <output>{w.appSettings.activity}/{String(w.appSettings.dirty)}/{w.settingsPending ?? 'none'}/{exits}</output>;}
createRoot(document.getElementById('root')!).render(<Harness/>);

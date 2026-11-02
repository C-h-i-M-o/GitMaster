import {createRoot} from 'react-dom/client';
import {mockIPC,mockWindows} from '@tauri-apps/api/mocks';
import App from '../../src/App';
import '../../src/styles.css';
import {defaultSettings} from '../../src/ui/settingsDraft';
import {mergeSettingsPatch} from '../../src/ui/settingsPatch';
import type {OperationRecord} from '../../src/types/git';
import type {SettingsPatch} from '../../src/types/settings';
let settings=defaultSettings(), revision=1;
let fail=false;
let fieldFailure:string|null=null;
let cacheFailure=false, cacheBytes=10066, createdTerminals=0;
Object.assign(window,{appearanceFixture:{failField:(field:string|null)=>{fieldFailure=field}, failSave:(value:boolean)=>{fail=value}, failCache:(value:boolean)=>{cacheFailure=value}, terminals:()=>createdTerminals}});
let content = "const greeting = '你好';\n", pendingContent = content, fileVersion = 1, operation: OperationRecord | null = null, fileSaves = 0;
Object.assign(window,{uxFixture:{saved:()=>({content,fileSaves,settings})}});
const commits=Array.from({length:1000},(_,i)=>({oid:(i+1).toString(16).padStart(40,'0'),parentOids:i<999?[(i+2).toString(16).padStart(40,'0')]:[],subject:`提交 ${i+1}：中文与长名称工作台测试`,authorName:'测试作者',authoredAt:'2026-09-24T00:00:00Z'}));
const tips=Array.from({length:12},(_,i)=>({refId:`ref-${i}`,name:i===0?'main':`feature/中文长名称分支-${i}-abcdefghijklmnopqrstuvwxyz`,kind:i<6?'local':'remote',oid:commits[0].oid}));
if(new URLSearchParams(location.search).has('shortRefs'))tips.splice(1);
const repository={repositoryId:'r',snapshotId:'s',rootPath:'/test',head:{kind:'branch',name:'main',oid:commits[0].oid},operations:[],changes:Array.from({length:10000},(_,i)=>({changeId:`c${i}`,path:`file-${i}.txt`,originalPath:null,indexStatus:'.',worktreeStatus:'M',kind:'tracked',binary:'unknown'}))};
Object.assign(window,{isTauri:true});
mockIPC(async (cmd,payload)=>{
 if(cmd==='plugin:window|set_theme')return null;
 if(cmd==='read_cache_usage'||cmd==='clear_content_cache'){if(cacheFailure)throw {code:'CACHE_UNAVAILABLE'};if(cmd==='clear_content_cache')cacheBytes=0;return {diskBytes:cacheBytes,contentBytes:Math.min(9856,cacheBytes)}}
 if(cmd==='create_terminal'){createdTerminals++;return {sessionId:'theme-terminal',repositoryId:'r',displayCwd:'/test',shellLabel:'测试 Shell'}}
 if(cmd==='read_terminal')return {sequence:1,bytes:(payload as {acknowledgedSequence:number}).acknowledgedSequence===0?Array.from(new TextEncoder().encode('保留终端输出\r\n')):[],finished:false,exitCode:null};
 if(cmd==='resize_terminal'||cmd==='write_terminal'||cmd==='close_terminal')return null;
 if(cmd==='choose_repository_path')return '/test';
 if(cmd==='read_commit_history')return {repositoryId:'r',graphSnapshotId:'g',tips,commits,nextCursor:null};
 if(cmd==='read_branches')return {repositoryId:'r',branches:tips.map((t,i)=>({...t,branchId:t.refId,current:i===0,occupiedByOtherWorktree:false,upstreamRefId:null}))};
 if(cmd==='read_log_settings')return {level:null,effectiveLevel:'info',directory:'/test/logs',available:true};
 if(cmd==='read_external_availability')return {vsCode:false,terminal:true};
 if(cmd==='open_diff_document')return {kind:'text',content:'@@ -1 +1 @@\n-old\n+new\n',truncated:false};
 if(cmd==='read_operation')return operation;
 if(cmd==='prepare_file_save'){ pendingContent=(payload as {content:string}).content; return {repositoryId:'r',snapshotId:repository.snapshotId,planId:'save',kind:'saveFile',head:repository.head,parentOids:[],paths:['a.ts'],author:null,message:null,target:null,warnings:[],expiresAt:Date.now()+60000}; }
 if(cmd==='execute_write'){ content=pendingContent; fileVersion++; fileSaves++; repository.snapshotId=`s${fileVersion}`; const handle={operationId:`save-${fileVersion}`,repositoryId:'r'}; operation={progress:{handle,sequence:1,kind:'saveFile',phase:'completed',counts:null,startedAt:Date.now()},result:{outcome:'succeeded',operationId:handle.operationId,kind:'saveFile',commitOid:null,branchName:null,clonePath:null,refresh:{status:'ready',state:repository}}}; return handle; }
 if(cmd==='read_app_settings')return {revision:String(revision),settings};
 if(cmd==='apply_settings_patch'){if(fieldFailure){await new Promise(resolve=>setTimeout(resolve,120));throw {code:'INVALID_INPUT',fieldErrors:[{field:fieldFailure,message:'该字段未通过验证，请检查后重试。'}]}}if(fail)throw {code:'SETTINGS_IO',fieldErrors:[]};const {expectedRevision,patch}=payload as {expectedRevision:string;patch:SettingsPatch};if(expectedRevision!==String(revision))throw {code:'STALE_SETTINGS',fieldErrors:[]};settings=mergeSettingsPatch(settings,patch);return {revision:String(++revision),settings};}
 if(cmd==='read_recent_projects')return [{rootPath:'/test',name:'GitMaster',lastOpenedAt:1},{rootPath:'/test/second',name:'第二个项目',lastOpenedAt:0}];
 if(cmd==='read_remotes')return {repositoryId:'r',remotes:[],branchUpstreams:[],lastFetchedAt:null};
 if(cmd==='read_write_context')return {repositoryId:'r',snapshotId:'s',capabilities:Object.fromEntries(['stage','unstage','commit','commitSelected','createBranch','switchBranch','fetch','push','integrate','finishMerge'].map(key=>[key,{status:'allowed'}]))};
 if(cmd==='read_project_file')return {kind:'text',content,truncated:false};
 if(cmd==='apply_app_settings'){if(fail)throw {code:'SETTINGS_IO',fieldErrors:[]};return {revision:'2',settings:(payload as {draft:unknown}).draft};}
 if(cmd==='detect_git')return {status:'ready',executablePath:'/test/git',version:'git version 2.49.0',source:'path'};
 if(cmd==='open_repository'||cmd==='read_repository_state')return repository;
 if(cmd==='read_repository_watch')return {repositoryId:'r',revision:0,reliable:true};
 if(cmd==='read_project_tree')return {repositoryId:'r',snapshotId:repository.snapshotId,treeId:`t${fileVersion}`,directoryId:'t',total:1,searchIncomplete:false,entries:[{id:'f',kind:'file',path:'a.ts',name:'a.ts',status:'tracked'}],nextOffset:null,truncated:false};
 if(cmd==='read_editable_file')return {fileId:'f',path:'a.ts',version:String(fileVersion),text:{content,bom:false,lineEnding:'lf',contentVersion:String(fileVersion)}};
 if(cmd==='read_conflicts')return {repositoryId:'r',mergeSessionId:'m',headOid:'a'.repeat(40),mergeHeadOids:['b'.repeat(40)],files:[{conflictId:'c',path:'conflict.ts',stageOids:{base:null,local:null,incoming:null},editorSupport:{status:'supported'}}]};
 if(cmd==='read_conflict_document')return {mergeSessionId:'m',conflictId:'c',base:'base',local:'local',incoming:'incoming',result:'原始',encoding:'utf8',lineEnding:'lf',fingerprint:'v'};
 if(cmd==='plugin:event|listen')return 1;
 if(cmd==='plugin:event|unlisten')return null;
 throw {code:'TEST_UNSUPPORTED',message:cmd,retryable:false};
});
mockWindows('main');
createRoot(document.getElementById('root')!).render(<App/>);

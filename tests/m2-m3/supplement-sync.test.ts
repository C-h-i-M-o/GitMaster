import assert from 'node:assert/strict';
import test from 'node:test';
import {createRemoteSyncController} from '../../src/hooks/remoteSyncController.ts';
import type {RemoteSyncApi} from '../../src/hooks/remoteSyncController.ts';
import type {RepositoryState,RemoteRelation,RemoteWriteRequest,OperationResult,OperationKind} from '../../src/types/git.ts';
/** 模拟真实终态刷新后快照与远端 ID 重新签发，禁止依赖固定 ID。 */
function fixture(relation:RemoteRelation='equal') {
 let repo:RepositoryState={repositoryId:'r',snapshotId:'s0',rootPath:'/fixture',head:{kind:'branch',name:'main',oid:'local'},operations:[],changes:[]};
 let generation=0;let configured=true;let exists=true;let failure:RemoteWriteRequest["kind"]|null=null;let unknown=false;let refreshFailure=false;let afterFetch:()=>void=()=>{};
 const requests:RemoteWriteRequest[]=[];
 const api:RemoteSyncApi={
  repository:()=>repo,blocked:()=>false,
  readRemotes:async()=>({repositoryId:'r',remotes:[{remoteId:`remote${generation}`,name:'team/origin',fetchDisplayUrl:'fixture',pushDisplayUrl:'fixture',fetchedBranchNames:exists?['release']:[]}],remoteBranches:exists?[{remoteBranchId:`branch${generation}`,name:'team/origin/release',oid:'remote'}]:[],branchUpstreams:[],lastFetchedAt:null}),
  readSyncTarget:async(id,snapshot)=>({repositoryId:id,snapshotId:snapshot,branchName:'main',upstream:configured?{status:'configured',remoteId:`remote${generation}`,targetBranchName:'release',remoteBranchId:exists?`branch${generation}`:null}:{status:'missing'}}),
  assessRemote:async(id,snapshot,branch)=>({repositoryId:id,snapshotId:snapshot,remoteBranchId:branch,localOid:'local',remoteOid:'remote',ahead:relation==='ahead'?1:0,behind:relation==='behind'?1:0,relation,observedAt:'now'}),
  execute:async(_source,request):Promise<OperationResult>=>{
   requests.push(request);
   const resultKind:OperationKind=request.kind==='fetchAll'?'fetch':request.kind==='publishBranch'||request.kind==='syncPush'?'push':request.kind==='syncFastForward'?'integrate':request.kind;
   if(request.kind===failure&&!refreshFailure)return {outcome:unknown?'unknown':'failed',operationId:'op',kind:resultKind,error:{code:unknown?'WRITE_OUTCOME_UNKNOWN':'NETWORK_UNAVAILABLE',retryable:false},cloneRecovery:null,refresh:{status:'notApplicable'}};
   if(request.kind==='fetchAll'){generation++;repo={...repo,snapshotId:`s${generation}`};afterFetch();}
   if(request.kind==='syncPush'){generation++;repo={...repo,snapshotId:`s${generation}`};}
   if(request.kind==='publishBranch')exists=true;
   if(request.kind==='setUpstream')configured=true;
   if(request.kind==='syncFastForward')repo={...repo,head:{kind:'branch',name:'main',oid:'remote'}};
   return {outcome:'succeeded',operationId:'op',kind:resultKind,commitOid:null,branchName:'main',clonePath:null,refresh:request.kind===failure&&refreshFailure?{status:'failed',error:{code:'GIT_EXECUTION_FAILED',retryable:false}}:{status:'ready',state:repo}};
  }
 };
 const controller=createRemoteSyncController(api);
 return {controller,api,requests,kinds:()=>requests.map(r=>r.kind),setup:(present:boolean)=>{configured=false;exists=present},fail:(kind:RemoteWriteRequest["kind"],isUnknown=false,refresh=false)=>{failure=kind;unknown=isUnknown;refreshFailure=refresh},afterFetch:(callback:()=>void)=>{afterFetch=callback},changeBranch:()=>{repo={...repo,head:{kind:'branch',name:'other',oid:'local'}}},dirty:()=>{repo={...repo,changes:[{changeId:'c',path:'dirty',originalPath:null,indexStatus:'.',worktreeStatus:'M',kind:'tracked',binary:'unknown'}]}}};
}
for(const [relation,expected] of [['equal',['fetchAll']],['ahead',['fetchAll','syncPush','fetchAll']],['behind',['fetchAll','syncFastForward']],['diverged',['fetchAll']]] as const) {
 test(`同步 ${relation} 仅执行当前上游对应动作`,async()=>{const f=fixture(relation);await f.controller.run();assert.deepEqual(f.kinds(),expected);assert.equal(f.controller.getSnapshot().error,null);assert.equal(f.controller.getSnapshot().diverged,relation==='diverged');});
}
for(const relation of ['unrelated','unknown'] as const)test(`${relation} 不自动写入`,async()=>{const f=fixture(relation);await f.controller.run();assert.deepEqual(f.kinds(),['fetchAll']);assert.ok(f.controller.getSnapshot().error);});
for(const unknown of [false,true])test(`获取${unknown?'结果未知':'失败'}后停止`,async()=>{const f=fixture('ahead');f.fail('fetchAll',unknown);await f.controller.run();assert.deepEqual(f.kinds(),['fetchAll']);assert.ok(f.controller.getSnapshot().error);});
test('获取后分支变化或工作区变脏不得推送',async()=>{for(const dirty of [false,true]){const f=fixture('ahead');f.afterFetch(dirty?f.dirty:f.changeBranch);await f.controller.run();assert.deepEqual(f.kinds(),['fetchAll']);assert.equal(f.controller.getSnapshot().error?.code,dirty?'WORKTREE_DIRTY':'STALE_REQUEST');}});
test('无上游等待用户明确选择，已有目标只设置上游',async()=>{const f=fixture();f.setup(true);await f.controller.run();assert.deepEqual(f.kinds(),[]);assert.ok(f.controller.getSnapshot().setup);await f.controller.configure('remote0','release',false);assert.deepEqual(f.kinds(),['fetchAll','setUpstream','fetchAll']);assert.equal(f.controller.getSnapshot().error,null);assert.deepEqual(f.requests[1],{kind:'setUpstream',remoteId:'remote1',targetBranchName:'release'});});
test('目标缺失且未允许创建时绝不发布',async()=>{const f=fixture();f.setup(false);await f.controller.run();await f.controller.configure('remote0','release',false);assert.deepEqual(f.kinds(),['fetchAll']);assert.equal(f.controller.getSnapshot().error?.code,'SYNC_TARGET_MISSING');});
test('首次发布成功后重新获取，再设置上游和同步',async()=>{const f=fixture();f.setup(false);await f.controller.run();await f.controller.configure('remote0','release',true);assert.deepEqual(f.kinds(),['fetchAll','publishBranch','fetchAll','setUpstream','fetchAll']);assert.equal(f.controller.getSnapshot().error,null);assert.deepEqual(f.requests[3],{kind:'setUpstream',remoteId:'remote2',targetBranchName:'release'});});
for(const kind of ['publishBranch','setUpstream'] as const)test(`${kind} 成功但刷新失败保留已完成事实`,async()=>{const f=fixture();f.setup(kind!=='publishBranch');f.fail(kind,false,true);await f.controller.run();await f.controller.configure('remote0','release',true);assert.equal(f.kinds().at(-1),kind);assert.ok(f.controller.getSnapshot().error);assert.match(f.controller.getSnapshot().notice??'',kind==='publishBranch'?/已发布/:/上游已设置/);});
test('双击合并且重置后迟到获取不得接续推送',async()=>{const f=fixture('ahead');const original=f.api.execute;let finish:()=>void=()=>{};f.api.execute=async(source,request)=>{if(request.kind==='fetchAll')await new Promise<void>(resolve=>finish=resolve);return original(source,request)};const first=f.controller.run();await Promise.resolve();await Promise.resolve();await f.controller.run();f.controller.reset();finish();await first;assert.deepEqual(f.kinds(),['fetchAll']);assert.equal(f.controller.getSnapshot().notice,null);assert.equal(f.controller.getSnapshot().error,null);});

/** 推送完成后重新取得远端能力，获取失败不撤销成功事实或触发重推。 */
test('推送后补充获取使用新 ID，失败保留已推送提示',async()=>{
 const f=fixture('ahead'); const execute=f.api.execute;
 f.api.execute=async(source,request)=>{const result=await execute(source,request);if(request.kind==='syncPush')f.fail('fetchAll');return result;};
 await f.controller.run();
 assert.deepEqual(f.kinds(),['fetchAll','syncPush','fetchAll']);
 assert.deepEqual(f.requests[2],{kind:'fetchAll',remoteId:'remote2'});
 assert.match(f.controller.getSnapshot().notice??'',/已推送/);
 assert.ok(f.controller.getSnapshot().error);
});
test('推送成功但本地刷新失败仍保留推送事实',async()=>{
 const f=fixture('ahead');f.fail('syncPush',false,true);await f.controller.run();
 assert.deepEqual(f.kinds(),['fetchAll','syncPush']);assert.match(f.controller.getSnapshot().notice??'',/已推送/);assert.ok(f.controller.getSnapshot().error);
});
test('推送后分支变化停止补充获取',async()=>{
 const f=fixture('ahead');const execute=f.api.execute;
 f.api.execute=async(source,request)=>{const result=await execute(source,request);if(request.kind==='syncPush')f.changeBranch();return result;};
 await f.controller.run();assert.deepEqual(f.kinds(),['fetchAll','syncPush']);assert.equal(f.controller.getSnapshot().error?.code,'STALE_REQUEST');assert.match(f.controller.getSnapshot().notice??'',/已推送/);
});
/** 推送后目标改变或控制器重置时不能继续使用旧同步链。 */
test('推送后上游变化不自动获取新目标',async()=>{
 const f=fixture('ahead');const execute=f.api.execute;const read=f.api.readSyncTarget;let pushed=false;
 f.api.execute=async(source,request)=>{const result=await execute(source,request);if(request.kind==='syncPush')pushed=true;return result;};
 f.api.readSyncTarget=async(id,snapshot)=>{const target=await read(id,snapshot);return pushed?{...target,upstream:{status:'configured',remoteId:'other',targetBranchName:'other',remoteBranchId:'other'}}:target;};
 await f.controller.run();assert.deepEqual(f.kinds(),['fetchAll','syncPush']);assert.equal(f.controller.getSnapshot().error?.code,'REMOTE_CHANGED');
});
test('补充获取迟到响应不覆盖重置状态',async()=>{
 const f=fixture('ahead');const execute=f.api.execute;let count=0;let release:()=>void=()=>{};let entered:()=>void=()=>{};const waiting=new Promise<void>(resolve=>entered=resolve);
 f.api.execute=async(source,request)=>{if(request.kind==='fetchAll'&&++count===2){entered();await new Promise<void>(resolve=>release=resolve);}return execute(source,request);};
 const pending=f.controller.run();await waiting;f.controller.reset();release();await pending;
 assert.deepEqual(f.kinds(),['fetchAll','syncPush','fetchAll']);assert.equal(f.controller.getSnapshot().notice,null);assert.equal(f.controller.getSnapshot().error,null);
});

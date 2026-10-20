import assert from 'node:assert/strict';
import test from 'node:test';
import {createRepositoryController} from '../../src/hooks/repositoryController.ts';
import {createRemoteController} from '../../src/hooks/remoteController.ts';
import type {RepositoryState} from '../../src/types/git.ts';
import type {LocalDiff,PagedDiff} from '../../src/types/diff.ts';
const repository:RepositoryState={repositoryId:'r',snapshotId:'s',rootPath:'/fixture',head:{kind:'unborn',name:'main'},operations:[],changes:[]};
const doc=(id:string):PagedDiff=>({kind:'paged',folds:[],documentId:id,rowCount:10000,hunkCount:1,additions:9999,deletions:0,truncated:false});

test('本地远端读取失效只重试一次，评估采用最新快照',async()=>{
 let calls=0;let snapshot='';
 const c=createRemoteController({readRemotes:async()=>{calls++;if(calls===1)throw {code:'STALE_REQUEST'};return {repositoryId:'r',remotes:[],remoteBranches:[{remoteBranchId:'b',name:'origin/main',oid:'a'}],branchUpstreams:[]};},assessRemote:async(_r,s)=>{snapshot=s;throw {code:'GIT_EXECUTION_FAILED'};}});
 c.setRepository(repository);await c.refresh();assert.equal(calls,2);assert.equal(c.getSnapshot().error,null);
 c.setRepository({...repository,snapshotId:'new'});await c.selectBranch('b');assert.equal(snapshot,'new');assert.equal(c.getSnapshot().error?.code,'GIT_EXECUTION_FAILED');
 const failed=createRemoteController({readRemotes:async()=>{calls++;throw {code:'STALE_REQUEST'};},assessRemote:async()=>{throw Error('不应评估')}});calls=0;failed.setRepository(repository);await failed.refresh();assert.equal(calls,2);assert.equal(failed.getSnapshot().loading,false);
});

test('刷新保留路径并按新文件身份重读完整上下文',async()=>{
 const old={...repository,changes:[{changeId:'old',path:'README.md',kind:'tracked',indexStatus:'.',worktreeStatus:'M'}]};
 const next={...old,snapshotId:'s2',changes:[{...old.changes[0],changeId:'new'}]};
 const requests:unknown[][]=[];
 let finish:(value:RepositoryState)=>void=()=>{};
 const c=createRepositoryController({openRepository:async()=>old,readRepositoryState:()=>new Promise(resolve=>finish=resolve),readFileDiff:async(...args)=>{requests.push(args);return doc(args[2]);},closeFileDiff:async()=>{}});
 await c.open('/fixture');await c.selectDiff('old','unstaged');
 const refresh=c.refresh();assert.equal(c.getSnapshot().selected?.changeId,'old');assert.equal(c.getSnapshot().diffLoading,true);
 finish(next);await refresh;await Promise.resolve();
 assert.equal(c.getSnapshot().selected?.changeId,'new');assert.equal(c.getSnapshot().diff?.kind,'paged');assert.equal(c.getSnapshot().diffLoading,false);
 assert.deepEqual(requests.map(args=>[args[1],args[2],args[4]]),[['s','old',1_000_000_000],['s2','new',1_000_000_000]]);
});

test('差异消失后保留文件路径并结束加载，展示无差异状态',async()=>{
 const old={...repository,changes:[{changeId:'old',path:'README.md',kind:'tracked',indexStatus:'.',worktreeStatus:'M'}]};
 const c=createRepositoryController({openRepository:async()=>old,readRepositoryState:async()=>({...repository,snapshotId:'new'}),readFileDiff:async()=>doc('opened'),closeFileDiff:async()=>{}});
 await c.open('/fixture');await c.selectDiff('old','unstaged');await c.refresh();
 assert.equal(c.getSnapshot().selected?.path,'README.md');assert.equal(c.getSnapshot().diffLoading,false);assert.deepEqual(c.getSnapshot().diff,{kind:'text',content:'',truncated:false});
});
test('迟到打开主动释放，关闭详情后不回填',async()=>{const pending:Array<(d:LocalDiff)=>void>=[];const closed:string[]=[];const c=createRepositoryController({openRepository:async()=>repository,readRepositoryState:async()=>repository,readFileDiff:()=>new Promise(r=>pending.push(r)),closeFileDiff:async(_r,_s,id)=>{closed.push(id)}});await c.open('/fixture');const first=c.selectDiff('a','staged');const second=c.selectDiff('b','unstaged');pending[1]!(doc('second'));await second;pending[0]!(doc('first'));await first;assert.equal(c.getSnapshot().diff?.kind,'paged');assert.deepEqual(closed,['first']);c.closeDiff();assert.deepEqual(closed,['first','second']);const third=c.selectDiff('c','untracked');c.closeDiff();pending[2]!(doc('third'));await third;assert.equal(c.getSnapshot().diff,null);assert.deepEqual(closed,['first','second','third']);});
test('刷新和清空释放当前文档',async()=>{const closed:string[]=[];let id=0;const c=createRepositoryController({openRepository:async()=>repository,readRepositoryState:async()=>repository,readFileDiff:async()=>doc(String(++id)),closeFileDiff:async(_r,_s,id)=>{closed.push(id)}});await c.open('/fixture');await c.selectDiff('a','staged');await c.refresh();await c.selectDiff('b','unstaged');c.clear();assert.deepEqual(closed,['1','2']);});
test('核实快照使在途 diff 失效且不标记仓库 stale',async()=>{let finish:(d:LocalDiff)=>void=()=>{};const c=createRepositoryController({openRepository:async()=>repository,readRepositoryState:async()=>repository,readFileDiff:()=>new Promise(r=>finish=r),closeFileDiff:async()=>{}});await c.open('/fixture');const task=c.selectDiff('a','staged');assert.equal(c.consumeVerifiedState({...repository,snapshotId:'s2'}),true);finish(doc('late'));await task;assert.equal(c.getSnapshot().diff,null);assert.equal(c.getSnapshot().stale,false);});
test('核实响应迟到不能覆盖新快照，刷新期间拒绝消费',async()=>{
 let resolveRefresh:(state:RepositoryState)=>void=()=>{};
 const c=createRepositoryController({openRepository:async()=>repository,readRepositoryState:()=>new Promise(resolve=>resolveRefresh=resolve),readFileDiff:async()=>doc('x'),closeFileDiff:async()=>{}});
 await c.open('/fixture');
 const next={...repository,snapshotId:'s2'};
 assert.equal(c.consumeVerifiedState(next,'s'),true);
 assert.equal(c.consumeVerifiedState({...next,snapshotId:'late'},'s'),false);
 const refreshing=c.refresh();
 assert.equal(c.consumeVerifiedState({...next,snapshotId:'late'},'s2'),false);
 resolveRefresh({...repository,snapshotId:'s3'}); await refreshing;
 assert.equal(c.getSnapshot().repository?.snapshotId,'s3');
});
test('过期差异只触发一次本地核实，不把仓库标为不可用',async()=>{
 let reads=0;
 const c=createRepositoryController({openRepository:async()=>repository,readRepositoryState:async()=>{reads++;return {...repository,snapshotId:'new'};},readFileDiff:async()=>{throw {code:'STALE_REQUEST'}},closeFileDiff:async()=>{}});
 await c.open('/fixture'); await c.selectDiff('a','unstaged'); await Promise.resolve();
 assert.equal(reads,1); assert.equal(c.getSnapshot().stale,false); assert.equal(c.getSnapshot().error,null);
});

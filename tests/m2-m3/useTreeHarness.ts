import { useState } from "react";
import { useFileTree } from "../../src/hooks/useFileTree";
import type { ProjectTreePage } from "../../src/types/git";
/** 夹具记录实际 Hook 请求并模拟分页、延迟和会话替换。 */
export function useTreeHarness() {
 const [root,setRoot]=useState<ProjectTreePage>({repositoryId:"r",snapshotId:"s",treeId:"t",directoryId:"root",entries:[{kind:"directory",id:"dir",path:"目录",name:"目录"}],nextOffset:null,total:1});
 const [selected,setSelected]=useState("");
 function openFile(id:string):()=>void{return()=>setSelected(id);}
 const [calls,setCalls]=useState<string[]>([]);
 async function load(id:string,offset:number):Promise<ProjectTreePage>{setCalls(old=>[...old,`${id}:${offset}`]);await new Promise(resolve=>setTimeout(resolve,50));return {...root,directoryId:id,entries:[{kind:"file",id:`file${offset}`,path:`目录/file${offset}.ts`,name:`file${offset}.ts`,status:"tracked"}],nextOffset:offset===0?1:null,total:2};}
 async function search(query:string,offset:number):Promise<ProjectTreePage>{setCalls(old=>[...old,`search:${query}:${offset}`]);await new Promise(resolve=>setTimeout(resolve,query==="slow"?800:10));return {...root,entries:[{kind:"file",id:query,path:`${query}.ts`,name:`${query}.ts`,status:"tracked"}],nextOffset:null,total:1};}
 function reset():void{setRoot({...root,treeId:"next",directoryId:"next-root",entries:[]});}
 return {tree:useFileTree(root,load,search),calls,reset,selected,openFile};
}

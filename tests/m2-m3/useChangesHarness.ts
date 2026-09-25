import { useState } from "react";
import type { DiffSide, FileChange } from "../../src/types/git";
const files:FileChange[]=Array.from({length:10000},(_,n)=>({changeId:`id-${n}`,path:`file-${n}.ts`,originalPath:null,indexStatus:n===0?"M":" ",worktreeStatus:"M",kind:n===1?"submodule":"tracked",binary:"text"}));
const groups={unstaged:files,staged:[files[0]!],untracked:[]};
/** 模拟同文件两侧与禁用边界，不执行 Git 写入。 */
export function useChangesHarness(){
 const [selected,setSelected]=useState<{stage:string[];unstage:string[]}>({stage:[],unstage:[]});
 const [blocked,setBlocked]=useState(false);
 const [opened,setOpened]=useState("");
 function toggle(kind:"stage"|"unstage",id:string):()=>void{return()=>setSelected(old=>({...old,[kind]:old[kind].includes(id)?old[kind].filter(value=>value!==id):[...old[kind],id]}));}
 function inspect(id:string,side:DiffSide):()=>void{return()=>setOpened(`${side}:${id}`);}
 function block():void{setBlocked(value=>!value);}
 return {groups,selected,blocked,opened,toggle,inspect,block};
}

import { useState } from "react";
import type { FileTreeNode } from "../../src/ui/fileTree";
const nodes:FileTreeNode[]=Array.from({length:10000},(_,n)=>({kind:"file",name:`file-${n}.ts`,path:`file-${n}.ts`,fileId:`id-${n}`,status:"tracked"}));
/** 用真实视口组件验证一万逻辑行，夹具不访问用户文件。 */
export function useVirtualTreeHarness(){
 const [selected,setSelected]=useState("");
 const [hidden,setHidden]=useState(false);
 function openFile(id:string):()=>void{return()=>setSelected(id);}
 function noop():void{}
 function falseValue():boolean{return false;}
 function action():()=>void{return noop;}
 function toggle():void{setHidden(value=>!value);}
 return {selected,hidden,openFile,toggle,tree:{nodes,isOpen:falseValue,toggle:action,more:action,hasMore:falseValue,loading:falseValue}};
}

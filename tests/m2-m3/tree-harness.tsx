import { createRoot } from "react-dom/client";
import { useTreeHarness } from "./useTreeHarness";
import { VirtualFileTree } from "../../src/components/VirtualFileTree";
import "../../src/styles.css";
/** 真实目录 Hook 与虚拟视图组合，数据来自可控异步夹具。 */
function Harness(){const {tree,calls,reset,selected,openFile}=useTreeHarness();return <><input aria-label="搜索" value={tree.query} onChange={tree.filter}/><button onClick={reset}>更换树</button><pre id="calls">{JSON.stringify(calls)}</pre><pre id="selected">{selected}</pre><div style={{height:400,width:400,display:"flex"}}><VirtualFileTree tree={tree} selectedPath="" openFile={openFile}/></div>{tree.error&&<p role="alert">{tree.error}</p>}</>;}
createRoot(document.getElementById("root")!).render(<Harness/>);

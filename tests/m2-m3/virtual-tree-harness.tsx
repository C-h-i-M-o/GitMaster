import { createRoot } from "react-dom/client";
import { VirtualFileTree } from "../../src/components/VirtualFileTree";
import { useVirtualTreeHarness } from "./useVirtualTreeHarness";
import "../../src/styles.css";
/** 一万文件的独立页面，计数与滚动检查使用实际 DOM。 */
function Harness(){const h=useVirtualTreeHarness();return <><button onClick={h.toggle}>隐藏或恢复</button><p id="selected">{h.selected}</p><div hidden={h.hidden} style={{height:400,width:400,display:h.hidden?"none":"flex"}}><VirtualFileTree tree={h.tree} selectedPath={h.selected.replace("id-","file-")+".ts"} openFile={h.openFile}/></div></>;}
createRoot(document.getElementById("root")!).render(<Harness/>);

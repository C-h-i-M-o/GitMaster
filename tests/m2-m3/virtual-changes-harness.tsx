import { createRoot } from "react-dom/client";
import { VirtualChanges } from "../../src/components/VirtualChanges";
import { useChangesHarness } from "./useChangesHarness";
import "../../src/styles.css";
/** 真实虚拟变更组件消费可检查状态，避免依赖真实仓库。 */
function Harness(){const h=useChangesHarness();return <><button onClick={h.block}>切换禁用</button><pre id="selected">{JSON.stringify(h.selected)}</pre><pre id="opened">{h.opened}</pre><div style={{height:400,width:440,display:"flex"}}><VirtualChanges groups={h.groups} selected={h.selected} blocked={h.blocked} toggle={h.toggle} inspect={h.inspect}/></div></>;}
createRoot(document.getElementById("root")!).render(<Harness/>);

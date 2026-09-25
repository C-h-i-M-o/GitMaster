import { createRoot } from "react-dom/client";
import FileEditor from "../../src/components/FileEditor";
import { defaultSettings } from "../../src/ui/settingsDraft";
import { useEditorHarness } from "./useEditorHarness";
import "../../src/styles.css";
/** 独立浏览器交互夹具，不访问真实仓库。 */
function Harness() {
  const h = useEditorHarness();
  return <><button onClick={h.toggle}>切换文档</button><button onClick={h.hide}>隐藏或恢复</button>
    <p>{h.active} 保存目标：{h.saved}</p>
    <div hidden={h.hidden} style={{ position: "relative", height: 480 }}><FileEditor repositoryId="test" tabs={h.tabs} activePath={h.active} preferences={defaultSettings().editor} edit={h.edit} save={h.save} /></div>
    <pre id="drafts">{JSON.stringify(h.tabs.map((tab) => ({ path: tab.document.path, draft: tab.draft })))}</pre></>;
}
createRoot(document.getElementById("root")!).render(<Harness />);

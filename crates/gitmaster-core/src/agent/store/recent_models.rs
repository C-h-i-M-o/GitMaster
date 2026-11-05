use super::*;
use serde::Deserialize;

/// 最近使用列表独立于默认模型，不改变其他聊天的模型选择。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecentModels {
    version: u8,
    ids: Vec<String>,
}

impl AgentStore {
    /// 只返回仍启用的最近模型，过期引用不展示给用户。
    pub fn recent_model_ids(&self) -> AgentResult<Vec<String>> {
        let _guard = self.lock()?;
        let settings = self.read_settings()?;
        Ok(self
            .read_recent_models()?
            .into_iter()
            .filter(|id| {
                settings
                    .models
                    .iter()
                    .any(|model| &model.id == id && model.enabled)
            })
            .collect())
    }
    /// 接受新一轮运行时更新顺序，去重且最多保留五项。
    pub fn remember_model(&self, id: &str) -> AgentResult<()> {
        valid_id(id)?;
        let _guard = self.lock()?;
        let settings = self.read_settings()?;
        if !settings
            .models
            .iter()
            .any(|model| model.id == id && model.enabled)
        {
            return Err(AgentError::new(
                "AGENT_PROFILE_MISSING",
                "该模型已禁用或删除。",
            ));
        }
        let mut ids = self.read_recent_models()?;
        ids.retain(|item| {
            item != id
                && settings
                    .models
                    .iter()
                    .any(|model| &model.id == item && model.enabled)
        });
        ids.insert(0, id.into());
        ids.truncate(5);
        atomic_json(
            &self.root.join("recent-models.json"),
            &RecentModels { version: 1, ids },
        )
    }
    /// 在已有目录锁内读取严格、有界的最近列表。
    fn read_recent_models(&self) -> AgentResult<Vec<String>> {
        let path = self.root.join("recent-models.json");
        regular_or_missing(&path)?;
        if !path.exists() {
            return Ok(vec![]);
        }
        let document: RecentModels = read_json(&path)?;
        if document.version != 1 || document.ids.len() > 5 {
            return Err(AgentError::corrupt());
        }
        let mut unique = std::collections::HashSet::new();
        for id in &document.ids {
            valid_id(id)?;
            if !unique.insert(id) {
                return Err(AgentError::corrupt());
            }
        }
        Ok(document.ids)
    }
}

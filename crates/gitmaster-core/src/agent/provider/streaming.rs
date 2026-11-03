use super::*;
use serde_json::json;
use std::collections::BTreeMap;

/// 各协议的流式累积器；只有协议结束标记齐全才提供可执行回复。
pub(super) struct Accumulator {
    protocol: ProviderProtocol,
    text: String,
    reasoning: String,
    refusal: String,
    calls: BTreeMap<usize, (String, String, String)>,
    finish_reason: Option<String>,
    response: Option<Value>,
    blocks: Vec<Value>,
    arguments: BTreeMap<usize, String>,
    ended: bool,
}
impl Accumulator {
    /// 每个模型请求使用全新累积状态，失败不会污染下一次请求。
    pub(super) fn new(protocol: ProviderProtocol) -> Self {
        Self {
            protocol,
            text: String::new(),
            reasoning: String::new(),
            refusal: String::new(),
            calls: BTreeMap::new(),
            finish_reason: None,
            response: None,
            blocks: vec![],
            arguments: BTreeMap::new(),
            ended: false,
        }
    }
    /// 消费完整 SSE 事件；eventsource-stream 负责字节分片、UTF-8 与行边界。
    pub(super) fn push(
        &mut self,
        data: &str,
        delta: &(dyn Fn(&str) + Send + Sync),
    ) -> AgentResult<bool> {
        if data == "[DONE]" {
            if self.protocol != ProviderProtocol::ChatCompletions || self.finish_reason.is_none() {
                return Err(protocol_error());
            }
            self.ended = true;
            return Ok(true);
        }
        let value: Value = serde_json::from_str(data).map_err(|_| protocol_error())?;
        if value.get("error").is_some() || value["type"] == "error" {
            return Err(protocol_error());
        }
        match self.protocol {
            ProviderProtocol::ChatCompletions => self.chat(value, delta)?,
            ProviderProtocol::Responses => match value["type"].as_str() {
                Some("response.output_text.delta") => {
                    if let Some(text) = value["delta"].as_str() {
                        delta(text);
                    }
                }
                Some("response.completed") => {
                    self.response = Some(value["response"].clone());
                    self.ended = true;
                }
                Some("response.failed" | "response.incomplete") => return Err(protocol_error()),
                _ => {}
            },
            ProviderProtocol::AnthropicMessages => self.anthropic(value, delta)?,
        }
        Ok(self.ended)
    }
    /// 按工具 index 累积参数，不提前解析或执行任意参数前缀。
    fn chat(&mut self, value: Value, delta: &(dyn Fn(&str) + Send + Sync)) -> AgentResult<()> {
        let choices = value["choices"].as_array().ok_or_else(protocol_error)?;
        let Some(choice) = choices.first() else {
            return Ok(());
        };
        let part = &choice["delta"];
        if let Some(text) = part["content"].as_str() {
            self.text.push_str(text);
            delta(text);
        }
        if let Some(text) = part["reasoning_content"].as_str() {
            self.reasoning.push_str(text);
        }
        if let Some(text) = part["refusal"].as_str() {
            self.refusal.push_str(text);
            delta(text);
        }
        if let Some(calls) = part["tool_calls"].as_array() {
            for call in calls {
                let index = call["index"].as_u64().ok_or_else(protocol_error)? as usize;
                if index >= 16 {
                    return Err(protocol_error());
                }
                let entry = self.calls.entry(index).or_default();
                if let Some(id) = call["id"].as_str() {
                    entry.0.push_str(id);
                }
                if let Some(name) = call["function"]["name"].as_str() {
                    entry.1.push_str(name);
                }
                if let Some(arguments) = call["function"]["arguments"].as_str() {
                    entry.2.push_str(arguments);
                }
            }
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish_reason = Some(reason.into());
        }
        Ok(())
    }
    /// 保留 Anthropic 思考签名与工具输入，工具块结束后才解析完整 JSON。
    fn anthropic(&mut self, value: Value, delta: &(dyn Fn(&str) + Send + Sync)) -> AgentResult<()> {
        match value["type"].as_str() {
            Some("message_start") => {
                if let Some(blocks) = value["message"]["content"].as_array() {
                    self.blocks = blocks.clone();
                }
            }
            Some("content_block_start") => {
                let index = value["index"].as_u64().ok_or_else(protocol_error)? as usize;
                if index != self.blocks.len() || index > 128 {
                    return Err(protocol_error());
                }
                self.blocks.push(value["content_block"].clone());
            }
            Some("content_block_delta") => {
                let index = value["index"].as_u64().ok_or_else(protocol_error)? as usize;
                let block = self.blocks.get_mut(index).ok_or_else(protocol_error)?;
                let part = &value["delta"];
                match part["type"].as_str() {
                    Some("text_delta") => {
                        let text = part["text"].as_str().ok_or_else(protocol_error)?;
                        append_block(block, "text", text)?;
                        delta(text);
                    }
                    Some("input_json_delta") => self
                        .arguments
                        .entry(index)
                        .or_default()
                        .push_str(part["partial_json"].as_str().ok_or_else(protocol_error)?),
                    Some("thinking_delta") => {
                        append_block(
                            block,
                            "thinking",
                            part["thinking"].as_str().ok_or_else(protocol_error)?,
                        )?;
                    }
                    Some("signature_delta") => {
                        append_block(
                            block,
                            "signature",
                            part["signature"].as_str().ok_or_else(protocol_error)?,
                        )?;
                    }
                    _ => return Err(protocol_error()),
                }
            }
            Some("content_block_stop") => {
                let index = value["index"].as_u64().ok_or_else(protocol_error)? as usize;
                if let Some(arguments) = self.arguments.remove(&index) {
                    let block = self.blocks.get_mut(index).ok_or_else(protocol_error)?;
                    block["input"] =
                        serde_json::from_str(&arguments).map_err(|_| protocol_error())?;
                }
            }
            Some("message_delta") => {
                self.finish_reason = value["delta"]["stop_reason"].as_str().map(str::to_owned)
            }
            Some("message_stop") => self.ended = true,
            Some("ping") => {}
            _ => return Err(protocol_error()),
        }
        Ok(())
    }
    /// 缺失结束标记或参数块未结束时明确拒绝，不将部分输出视为完成。
    pub(super) fn finish(self) -> AgentResult<ModelReply> {
        if !self.ended || !self.arguments.is_empty() {
            return Err(protocol_error());
        }
        let raw = match self.protocol {
            ProviderProtocol::Responses => self.response.ok_or_else(protocol_error)?,
            ProviderProtocol::AnthropicMessages => {
                json!({"role":"assistant","content":self.blocks,"stop_reason":self.finish_reason})
            }
            ProviderProtocol::ChatCompletions => {
                let calls: Vec<Value> = self.calls.into_values().map(|(id,name,args)| json!({"id":id,"type":"function","function":{"name":name,"arguments":args}})).collect();
                let mut message = json!({"role":"assistant","content":self.text});
                if !calls.is_empty() {
                    message["tool_calls"] = json!(calls);
                }
                if !self.reasoning.is_empty() {
                    message["reasoning_content"] = json!(self.reasoning);
                }
                if !self.refusal.is_empty() {
                    message["refusal"] = json!(self.refusal);
                }
                json!({"choices":[{"message":message,"finish_reason":self.finish_reason}]})
            }
        };
        parse_response(self.protocol, raw)
    }
}

/// 直接扩展协议块的字符串，避免每个文本、思考或签名片段都重建完整内容。
fn append_block(block: &mut Value, field: &str, text: &str) -> AgentResult<()> {
    let value = &mut block[field];
    if value.is_null() {
        *value = Value::String(String::new());
    }
    match value {
        Value::String(current) => {
            current.push_str(text);
            Ok(())
        }
        _ => Err(protocol_error()),
    }
}

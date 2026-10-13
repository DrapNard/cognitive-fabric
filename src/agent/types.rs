use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub parameters: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: Function,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}
#[derive(Clone, Debug)]
pub struct Completion {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub model: String,
    pub finish_reason: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn valid_call(c: &ToolCall) -> Result<()> {
    if c.kind != "function"
        || !name(&c.function.name)
        || !name(&c.id)
        || c.function.arguments.len() > 32000
    {
        bail!("Invalid function call")
    }
    let args: Value = serde_json::from_str(&c.function.arguments)?;
    if !args.is_object() {
        bail!("Function arguments must be a JSON object")
    }
    Ok(())
}
impl Turn {
    pub fn validate(&self) -> Result<()> {
        if self.messages.is_empty()
            || self.messages.len() > 256
            || self.tools.len() > 64
            || serde_json::to_vec(self)?.len() > 512 * 1024
            || self
                .temperature
                .is_some_and(|n| !n.is_finite() || !(0.0..=2.0).contains(&n))
        {
            bail!("Conversation exceeds limits")
        }
        let mut tools = BTreeSet::new();
        for t in &self.tools {
            if t.kind != "function"
                || !name(&t.function.name)
                || !tools.insert(t.function.name.as_str())
                || t.function.description.len() > 16000
                || !t.function.parameters.is_object()
                || serde_json::to_vec(&t.function.parameters)?.len() > 32000
            {
                bail!("Invalid or duplicate tool definition")
            }
        }
        if let Some(choice) = &self.tool_choice {
            if !matches!(choice.as_str(), Some("auto" | "none" | "required"))
                && !(choice["type"] == "function"
                    && choice["function"]["name"]
                        .as_str()
                        .is_some_and(|n| tools.contains(n)))
            {
                bail!("Invalid tool choice")
            }
            if self.tools.is_empty() && choice != "none" {
                bail!("Tool choice requires definitions")
            }
        }
        let mut pending = BTreeMap::new();
        let mut used = BTreeSet::new();
        for m in &self.messages {
            if m.content.as_ref().is_some_and(|s| s.len() > 256 * 1024) || m.tool_calls.len() > 16 {
                bail!("Message exceeds limits")
            }
            match m.role.as_str() {
                "system" | "developer" | "user" => {
                    if !pending.is_empty()
                        || !m.tool_calls.is_empty()
                        || m.tool_call_id.is_some()
                        || m.content.is_none()
                    {
                        bail!("Invalid instruction or user message")
                    }
                }
                "assistant" => {
                    if !pending.is_empty()
                        || m.tool_call_id.is_some()
                        || (m.content.is_none() && m.tool_calls.is_empty())
                    {
                        bail!("Invalid assistant message")
                    }
                    for c in &m.tool_calls {
                        valid_call(c)?;
                        if !used.insert(c.id.clone()) {
                            bail!("Duplicate tool call identity")
                        }
                        pending.insert(c.id.clone(), c.function.name.clone());
                    }
                }
                "tool" => {
                    if !m.tool_calls.is_empty()
                        || m.content.is_none()
                        || m.tool_call_id
                            .as_ref()
                            .and_then(|id| pending.remove(id))
                            .is_none()
                    {
                        bail!("Uncorrelated tool result")
                    }
                }
                _ => bail!("Unsupported conversation role"),
            }
        }
        if !pending.is_empty()
            || self
                .messages
                .last()
                .is_some_and(|m| matches!(m.role.as_str(), "assistant" | "system" | "developer"))
        {
            bail!("Conversation must end with user input or correlated tool results")
        }
        Ok(())
    }
}
impl Completion {
    pub fn validate(&self, request: &Turn, max_tools: u32) -> Result<()> {
        if self.model.is_empty()
            || self.model.len() > 256
            || (request.parallel_tool_calls == Some(false) && self.tool_calls.len() > 1)
            || self.tool_calls.len() > 16
            || self.tool_calls.len() > max_tools as usize
            || self.content.as_ref().is_some_and(|s| s.len() > 96000)
            || !matches!(
                self.finish_reason.as_str(),
                "stop" | "length" | "tool_calls"
            )
        {
            bail!("Invalid completion or tool proposal budget exceeded")
        }
        if self.tool_calls.is_empty() {
            if self.content.as_deref().is_none_or(|s| s.trim().is_empty())
                || self.finish_reason == "tool_calls"
                || request
                    .tool_choice
                    .as_ref()
                    .is_some_and(|v| v == "required" || v.is_object())
            {
                bail!("Missing required completion or tool call")
            }
        } else {
            if self.finish_reason != "tool_calls"
                || request.tool_choice.as_ref().is_some_and(|v| v == "none")
            {
                bail!("Unexpected tool calls")
            }
            let mut seen = BTreeSet::new();
            for c in &self.tool_calls {
                valid_call(c)?;
                if request
                    .messages
                    .iter()
                    .flat_map(|m| &m.tool_calls)
                    .any(|old| old.id == c.id)
                    || !seen.insert(&c.id)
                    || !request
                        .tools
                        .iter()
                        .any(|t| t.function.name == c.function.name)
                    || request
                        .tool_choice
                        .as_ref()
                        .filter(|v| v.is_object())
                        .is_some_and(|v| v["function"]["name"] != c.function.name)
                {
                    bail!("Undeclared or duplicate tool call")
                }
            }
        }
        Ok(())
    }
}

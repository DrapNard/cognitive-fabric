use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Maximum number of client-owned function definitions accepted in one turn.
/// The catalog is forwarded to the configured model; it grants no kernel rights.
pub const MAX_CLIENT_TOOLS: usize = 128;

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
    /// Per-turn provider reasoning hint. This selects an effort level for the
    /// configured model; it never selects a different model or changes budgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
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
/// Supported request hints for OpenAI-compatible provider effort controls.
/// Provider compatibility remains an upstream concern and errors are surfaced.
pub fn valid_reasoning_effort(effort: &str) -> bool {
    matches!(effort, "none" | "low" | "medium" | "high")
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
pub(super) fn valid_call(c: &ToolCall) -> Result<()> {
    if c.kind != "function"
        || !name(&c.function.name)
        || !name(&c.id)
        || c.function.arguments.len() > 32000
    {
        bail!("Invalid function call")
    }
    let args = crate::strict_json::value(c.function.arguments.as_bytes(), 32000)?;
    if !args.is_object() {
        bail!("Function arguments must be a JSON object")
    }
    Ok(())
}
impl Turn {
    pub fn validate(&self) -> Result<()> {
        if self.messages.is_empty()
            || self.messages.len() > 256
            || self.tools.len() > MAX_CLIENT_TOOLS
            || serde_json::to_vec(self)?.len() > 512 * 1024
            || self
                .temperature
                .is_some_and(|n| !n.is_finite() || !(0.0..=2.0).contains(&n))
        {
            bail!("Conversation exceeds limits")
        }
        if self
            .reasoning_effort
            .as_deref()
            .is_some_and(|effort| !valid_reasoning_effort(effort))
        {
            bail!("Unsupported provider reasoning effort")
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
                && !choice.as_object().is_some_and(|object| {
                    object.len() == 2
                        && object.get("type").is_some_and(|kind| kind == "function")
                        && object
                            .get("function")
                            .and_then(Value::as_object)
                            .is_some_and(|function| {
                                function.len() == 1
                                    && function
                                        .get("name")
                                        .and_then(Value::as_str)
                                        .is_some_and(|name| tools.contains(name))
                            })
                })
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

#[cfg(test)]
mod duplicate_argument_tests {
    use super::*;
    #[test]
    fn rejects_duplicate_arguments_in_proposals_and_correlated_history() {
        let base = Turn {
            messages: vec![Message {
                role: "user".into(),
                content: Some("Read a file".into()),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            tools: vec![Tool {
                kind: "function".into(),
                function: Function {
                    name: "read".into(),
                    description: String::new(),
                    parameters: serde_json::json!({"type":"object"}),
                    strict: None,
                },
            }],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            parallel_tool_calls: None,
            reasoning_effort: None,
        };
        for args in [
            r#"{"path":"a","path":"b"}"#,
            r#"{"nested":{"path":"a","\u0070ath":"b"}}"#,
            r#"{"path":"a","other":"b"}"#,
        ] {
            let valid = args.contains("other");
            let call = ToolCall {
                id: "read_1".into(),
                kind: "function".into(),
                function: FunctionCall {
                    name: "read".into(),
                    arguments: args.into(),
                },
            };
            let completion = Completion {
                content: None,
                tool_calls: vec![call.clone()],
                model: "fixture".into(),
                finish_reason: "tool_calls".into(),
                input_tokens: None,
                output_tokens: None,
            };
            assert_eq!(completion.validate(&base, 1).is_ok(), valid);
            let mut history = base.clone();
            history.messages.extend([
                Message {
                    role: "assistant".into(),
                    content: None,
                    tool_calls: vec![call],
                    tool_call_id: None,
                },
                Message {
                    role: "tool".into(),
                    content: Some("Result".into()),
                    tool_calls: vec![],
                    tool_call_id: Some("read_1".into()),
                },
            ]);
            assert_eq!(history.validate().is_ok(), valid);
        }
    }
}

#[cfg(test)]
mod tool_choice_shape_tests {
    use super::*;
    #[test]
    fn named_tool_choices_reject_unknown_controls_and_preserve_supported_shapes() {
        let mut turn:Turn=serde_json::from_value(serde_json::json!({"messages":[{"role":"user","content":"Read"}],"tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object"}}}]})).unwrap();
        for choice in [
            serde_json::json!("auto"),
            serde_json::json!("none"),
            serde_json::json!("required"),
            serde_json::json!({"type":"function","function":{"name":"read"}}),
        ] {
            turn.tool_choice = Some(choice);
            turn.validate().unwrap();
        }
        for choice in [
            serde_json::json!({"type":"function","function":{"name":"read"},"permissions":["shell"]}),
            serde_json::json!({"type":"function","function":{"name":"read","strict":true}}),
            serde_json::json!({"type":"function","function":{"name":"missing"}}),
            serde_json::json!({"type":"function","function":["read"]}),
            serde_json::json!(true),
        ] {
            turn.tool_choice = Some(choice);
            assert!(turn.validate().is_err());
        }
    }
}

#[cfg(test)]
mod client_tool_catalog_limits {
    use super::*;

    fn turn_with_tools(count: usize) -> Turn {
        let mut turn: Turn = serde_json::from_value(serde_json::json!({
            "messages": [{"role":"user","content":"Use the available tools"}]
        }))
        .unwrap();
        turn.tools = (0..count)
            .map(|index| Tool {
                kind: "function".into(),
                function: Function {
                    name: format!("tool_{index}"),
                    description: "Client supplied capability".into(),
                    parameters: serde_json::json!({"type":"object"}),
                    strict: None,
                },
            })
            .collect();
        turn
    }

    #[test]
    fn accepts_the_full_client_tool_catalog_and_rejects_the_next_entry() {
        assert_eq!(MAX_CLIENT_TOOLS, 128);
        assert!(turn_with_tools(MAX_CLIENT_TOOLS).validate().is_ok());
        assert!(turn_with_tools(MAX_CLIENT_TOOLS + 1).validate().is_err());
    }
}

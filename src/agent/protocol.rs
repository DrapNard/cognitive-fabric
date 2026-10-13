//! Verified native-conversation packets for future managed worker transport.
//! Decoding establishes bounded data only, never task authority or live deadlines.
use super::{Completion, Function, FunctionCall, Message, Tool, ToolCall, Turn};
use anyhow::{bail, Result};
use serde_json::json;
#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/agent_generated.rs"));
}
use generated::agent as fb;
pub const MAX_FRAME: usize = 256 * 1024;
const MAX_COPIED: usize = 512 * 1024;
const ROLES: [&str; 5] = ["system", "developer", "user", "assistant", "tool"];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub correlation: String,
    pub task: String,
    pub expires_at_ms: u64,
}
pub enum Payload {
    Request { turn: Turn, allocation: usize },
    Response(Completion),
    Error(String),
}
pub struct Packet {
    pub identity: Identity,
    pub payload: Payload,
}
struct Budget(usize);
impl Budget {
    fn text(&mut self, s: &str, max: usize) -> Result<String> {
        if s.len() > max || s.len() > self.0 {
            bail!("Native packet string or aggregate copy limit exceeded")
        }
        self.0 -= s.len();
        Ok(s.into())
    }
    fn optional(&mut self, s: Option<&str>, max: usize) -> Result<Option<String>> {
        s.map(|s| self.text(s, max)).transpose()
    }
}
fn call_table(c: &ToolCall) -> fb::ToolCallT {
    fb::ToolCallT {
        id: c.id.clone(),
        function: Box::new(fb::FunctionCallT {
            name: c.function.name.clone(),
            arguments: c.function.arguments.clone(),
        }),
    }
}
fn calls(
    value: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::ToolCall<'_>>>>,
    budget: &mut Budget,
) -> Result<Vec<ToolCall>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.len() > 16 {
        bail!("Too many native tool calls")
    }
    value
        .iter()
        .map(|c| {
            let f = c.function();
            let c = ToolCall {
                id: budget.text(c.id(), 64)?,
                kind: "function".into(),
                function: FunctionCall {
                    name: budget.text(f.name(), 64)?,
                    arguments: budget.text(f.arguments(), 32000)?,
                },
            };
            super::types::valid_call(&c)?;
            Ok(c)
        })
        .collect()
}
fn choice(turn: &Turn) -> Result<(fb::Choice, Option<String>)> {
    Ok(match turn.tool_choice.as_ref() {
        None => (fb::Choice::Unspecified, None),
        Some(v) if v == "auto" => (fb::Choice::Auto, None),
        Some(v) if v == "none" => (fb::Choice::None, None),
        Some(v) if v == "required" => (fb::Choice::Required, None),
        Some(v) => {
            let name = v["function"]["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid named choice"))?;
            if *v != json!({"type":"function","function":{"name":name}}) {
                bail!("Unknown named-choice fields")
            }
            (fb::Choice::Named, Some(name.into()))
        }
    })
}
pub fn encode(packet: &Packet) -> Result<Vec<u8>> {
    if packet.identity.correlation.len() > 64 || packet.identity.task.len() > 64 {
        bail!("Native identity exceeds bounds")
    }
    uuid::Uuid::parse_str(&packet.identity.correlation)?;
    uuid::Uuid::parse_str(&packet.identity.task)?;
    match &packet.payload {
        Payload::Response(c) => {
            if c.tool_calls.len() > 16
                || c.model.len() > 256
                || c.content.as_ref().is_some_and(|s| s.len() > 96000)
            {
                bail!("Native response exceeds limits")
            }
            for call in &c.tool_calls {
                super::types::valid_call(call)?;
            }
        }
        Payload::Error(e) if e.len() > 256 => bail!("Native error exceeds limits"),
        _ => (),
    }
    let mut root = fb::PacketT {
        version: 1,
        correlation_id: packet.identity.correlation.clone(),
        task_id: packet.identity.task.clone(),
        sender: "jev-r".into(),
        destination: "inference.native".into(),
        module_version: "1.0.0".into(),
        expires_at_ms: packet.identity.expires_at_ms,
        ..Default::default()
    };
    match &packet.payload {
        Payload::Request { turn, allocation } => {
            turn.validate()?;
            let (choice, choice_name) = choice(turn)?;
            let messages = turn
                .messages
                .iter()
                .map(|m| {
                    Ok(fb::MessageT {
                        role: fb::Role(
                            ROLES
                                .iter()
                                .position(|r| *r == m.role)
                                .ok_or_else(|| anyhow::anyhow!("Unknown native role"))?
                                as u8,
                        ),
                        content: m.content.clone(),
                        tool_calls: Some(m.tool_calls.iter().map(call_table).collect()),
                        tool_call_id: m.tool_call_id.clone(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let tools = turn
                .tools
                .iter()
                .map(|t| {
                    Ok(fb::ToolT {
                        function: Box::new(fb::FunctionT {
                            name: t.function.name.clone(),
                            description: t.function.description.clone(),
                            parameters_json: serde_json::to_string(&t.function.parameters)?,
                            strict: t.function.strict,
                        }),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            root.request = Some(Box::new(fb::RequestT {
                allocation: u32::try_from(*allocation)?,
                turn: Box::new(fb::TurnT {
                    messages,
                    tools: Some(tools),
                    choice,
                    choice_name,
                    max_tokens: turn.max_tokens.map(u32::try_from).transpose()?,
                    temperature: turn.temperature,
                    parallel_tool_calls: turn.parallel_tool_calls,
                }),
            }));
        }
        Payload::Response(c) => {
            root.response = Some(Box::new(fb::ResponseT {
                content: c.content.clone(),
                tool_calls: Some(c.tool_calls.iter().map(call_table).collect()),
                model: c.model.clone(),
                finish: match c.finish_reason.as_str() {
                    "stop" => fb::Finish::Stop,
                    "length" => fb::Finish::Length,
                    "tool_calls" => fb::Finish::ToolCalls,
                    _ => bail!("Unknown native finish reason"),
                },
                input_tokens: c.input_tokens,
                output_tokens: c.output_tokens,
            }))
        }
        Payload::Error(e) => root.error = Some(e.clone()),
    }
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let root = root.pack(&mut builder);
    builder.finish(root, Some("COGA"));
    let bytes = builder.finished_data();
    // The sender obeys precisely the receiver's semantic and allocation limits.
    decode(bytes)?;
    Ok(bytes.to_vec())
}
pub fn decode(bytes: &[u8]) -> Result<Packet> {
    if bytes.len() < 8
        || bytes.len() > MAX_FRAME
        || !flatbuffers::buffer_has_identifier(bytes, "COGA", false)
    {
        bail!("Invalid native packet frame")
    }
    let options = flatbuffers::VerifierOptions {
        max_depth: 32,
        max_tables: 4096,
        max_apparent_size: 1024 * 1024,
        ..Default::default()
    };
    let root = flatbuffers::root_with_opts::<fb::Packet>(&options, bytes)?;
    if root.version() != 1
        || root.sender() != "jev-r"
        || root.destination() != "inference.native"
        || root.module_version() != "1.0.0"
        || root.expires_at_ms() == 0
    {
        bail!("Unsupported native packet identity or version")
    }
    if [
        root.request().is_some(),
        root.response().is_some(),
        root.error().is_some(),
    ]
    .into_iter()
    .filter(|v| *v)
    .count()
        != 1
    {
        bail!("Ambiguous native payload")
    }
    let mut budget = Budget(MAX_COPIED);
    let identity = Identity {
        correlation: budget.text(root.correlation_id(), 64)?,
        task: budget.text(root.task_id(), 64)?,
        expires_at_ms: root.expires_at_ms(),
    };
    uuid::Uuid::parse_str(&identity.correlation)?;
    uuid::Uuid::parse_str(&identity.task)?;
    let payload = if let Some(r) = root.request() {
        if r.allocation() == 0 || r.allocation() > 16384 {
            bail!("Invalid native allocation")
        }
        let t = r.turn();
        let messages = t.messages();
        let tools = t.tools();
        if messages.is_empty() || messages.len() > 256 || tools.is_some_and(|v| v.len() > 64) {
            bail!("Native vector count exceeds limits")
        }
        let messages = messages
            .iter()
            .map(|m| {
                Ok(Message {
                    role: ROLES
                        .get(m.role().0 as usize)
                        .ok_or_else(|| anyhow::anyhow!("Unknown native role"))?
                        .to_string(),
                    content: budget.optional(m.content(), 256 * 1024)?,
                    tool_calls: calls(m.tool_calls(), &mut budget)?,
                    tool_call_id: budget.optional(m.tool_call_id(), 64)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let tools = tools
            .map(|tools| {
                tools
                    .iter()
                    .map(|t| {
                        let f = t.function();
                        let raw = budget.text(f.parameters_json(), 32000)?;
                        Ok(Tool {
                            kind: "function".into(),
                            function: Function {
                                name: budget.text(f.name(), 64)?,
                                description: budget.text(f.description(), 16000)?,
                                parameters: serde_json::from_str(&raw)?,
                                strict: f.strict(),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let tool_choice = match (t.choice(), t.choice_name()) {
            (fb::Choice::Unspecified, None) => None,
            (fb::Choice::Auto, None) => Some(json!("auto")),
            (fb::Choice::None, None) => Some(json!("none")),
            (fb::Choice::Required, None) => Some(json!("required")),
            (fb::Choice::Named, Some(name)) => {
                Some(json!({"type":"function","function":{"name":budget.text(name,64)?}}))
            }
            _ => bail!("Invalid native tool-choice shape"),
        };
        let turn = Turn {
            messages,
            tools,
            tool_choice,
            max_tokens: t.max_tokens().map(|n| n as usize),
            temperature: t.temperature(),
            parallel_tool_calls: t.parallel_tool_calls(),
        };
        turn.validate()?;
        if turn
            .max_tokens
            .is_some_and(|n| n != r.allocation() as usize)
        {
            bail!("Native allocation disagrees with the requested ceiling")
        }
        Payload::Request {
            turn,
            allocation: r.allocation() as usize,
        }
    } else if let Some(r) = root.response() {
        let tool_calls = calls(r.tool_calls(), &mut budget)?;
        let mut ids = std::collections::BTreeSet::new();
        if tool_calls.iter().any(|c| !ids.insert(&c.id)) {
            bail!("Duplicate native output call IDs")
        }
        let content = budget.optional(r.content(), 96000)?;
        let model = budget.text(r.model(), 256)?;
        let finish_reason = match r.finish() {
            fb::Finish::Stop => "stop",
            fb::Finish::Length => "length",
            fb::Finish::ToolCalls => "tool_calls",
            _ => bail!("Unknown native finish reason"),
        }
        .into();
        if model.is_empty()
            || (tool_calls.is_empty()
                && (content.as_deref().is_none_or(|s| s.trim().is_empty())
                    || r.finish() == fb::Finish::ToolCalls))
            || (!tool_calls.is_empty() && r.finish() != fb::Finish::ToolCalls)
        {
            bail!("Invalid native response shape")
        }
        Payload::Response(Completion {
            content,
            tool_calls,
            model,
            finish_reason,
            input_tokens: r.input_tokens(),
            output_tokens: r.output_tokens(),
        })
    } else {
        let e = budget.text(root.error().unwrap(), 256)?;
        if e.is_empty() {
            bail!("Empty native error")
        };
        Payload::Error(e)
    };
    Ok(Packet { identity, payload })
}

#[cfg(test)]
#[path = "protocol/tests.rs"]
mod regression;

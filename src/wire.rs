//! Verified FlatBuffers envelopes. No untrusted bytes are read using unchecked APIs.
use crate::engine::Outcome;
use anyhow::{bail, Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/cognitive_generated.rs"));
}
use generated::cognitive as fb;

const MAX_FRAME: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    Status,
    CancelActive,
    Stop,
}

#[derive(Clone, Debug)]
pub enum Message {
    Control {
        id: String,
        action: ControlAction,
    },
    ControlReply {
        id: String,
        active: bool,
        accepted: bool,
    },
    Ask {
        id: String,
        query: String,
        mode: String,
        conversation_ref: Option<String>,
        explain: bool,
    },
    Reply {
        id: String,
        outcome: Outcome,
    },
    Error {
        id: String,
        message: String,
    },
}

pub fn encode(message: &Message) -> Result<Vec<u8>> {
    if let Message::Ask {
        query,
        conversation_ref,
        ..
    } = message
    {
        if query.trim().is_empty() || query.len() > 12_000 {
            bail!("Query is empty or exceeds 12,000 bytes")
        }
        if let Some(reference) = conversation_ref {
            uuid::Uuid::parse_str(reference)?;
        }
    }
    let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let id = match message {
        Message::Ask { id, .. }
        | Message::Reply { id, .. }
        | Message::Error { id, .. }
        | Message::Control { id, .. }
        | Message::ControlReply { id, .. } => id,
    };
    if id.is_empty() || id.len() > 128 {
        bail!("Invalid request correlation ID")
    }
    let request_id = builder.create_string(id);
    let mut args = fb::EnvelopeArgs {
        version: 1,
        request_id: Some(request_id),
        ..Default::default()
    };
    match message {
        Message::Control { action, .. } => {
            args.kind = fb::MessageKind::Control;
            args.control = Some(fb::ControlRequest::create(
                &mut builder,
                &fb::ControlRequestArgs {
                    action: match action {
                        ControlAction::Status => fb::ControlAction::Status,
                        ControlAction::CancelActive => fb::ControlAction::CancelActive,
                        ControlAction::Stop => fb::ControlAction::Stop,
                    },
                },
            ));
        }
        Message::ControlReply {
            active, accepted, ..
        } => {
            args.kind = fb::MessageKind::ControlResponse;
            args.control_response = Some(fb::ControlResponse::create(
                &mut builder,
                &fb::ControlResponseArgs {
                    active: *active,
                    accepted: *accepted,
                },
            ));
        }
        Message::Ask {
            query,
            mode,
            conversation_ref,
            explain,
            ..
        } => {
            args.kind = fb::MessageKind::Request;
            let q = builder.create_string(query);
            let mode = match mode.as_str() {
                "fast" => fb::ReasoningMode::Fast,
                "standard" => fb::ReasoningMode::Standard,
                "deep" => fb::ReasoningMode::Deep,
                "research" => fb::ReasoningMode::Research,
                _ => bail!("Unknown reasoning mode"),
            };
            let reference = conversation_ref
                .as_ref()
                .map(|reference| builder.create_string(reference));
            let ask = fb::AskRequest::create(
                &mut builder,
                &fb::AskRequestArgs {
                    query: Some(q),
                    mode,
                    conversation_ref: reference,
                    explain: *explain,
                },
            );
            args.request = Some(ask);
        }
        Message::Reply { outcome, .. } => {
            args.kind = fb::MessageKind::Response;
            let task_id = builder.create_string(&outcome.task_id);
            let answer = builder.create_string(&outcome.answer);
            let status = builder.create_string(&outcome.status);
            let evidence = outcome
                .evidence
                .iter()
                .map(|s| builder.create_string(s))
                .collect::<Vec<_>>();
            let evidence = builder.create_vector(&evidence);
            let skills = outcome
                .skills
                .iter()
                .map(|s| builder.create_string(s))
                .collect::<Vec<_>>();
            let skills = builder.create_vector(&skills);
            let resources = fb::ResourceUsage::create(
                &mut builder,
                &fb::ResourceUsageArgs {
                    reserved_output_tokens: outcome.resources.reserved_output_tokens as u64,
                    prompt_bytes: outcome.resources.prompt_bytes as u64,
                    input_tokens: outcome.resources.input_tokens,
                    output_tokens: outcome.resources.output_tokens,
                    inference_ms: outcome.resources.inference_ms,
                    tool_calls: outcome.resources.tool_calls,
                },
            );
            let stop_reason = builder.create_string(&outcome.stop_reason);
            let reply = fb::AskResponse::create(
                &mut builder,
                &fb::AskResponseArgs {
                    task_id: Some(task_id),
                    answer: Some(answer),
                    status: Some(status),
                    model_calls: outcome.calls,
                    cycles: outcome.cycles,
                    elapsed_ms: outcome.elapsed_ms,
                    evidence: Some(evidence),
                    skills: Some(skills),
                    resources: Some(resources),
                    stop_reason: Some(stop_reason),
                },
            );
            args.response = Some(reply);
        }
        Message::Error { message, .. } => {
            args.kind = fb::MessageKind::Error;
            let detail = builder.create_string(message);
            let error = fb::ProtocolError::create(
                &mut builder,
                &fb::ProtocolErrorArgs {
                    message: Some(detail),
                },
            );
            args.error = Some(error);
        }
    }
    let root = fb::Envelope::create(&mut builder, &args);
    builder.finish(root, Some("COGF"));
    let body = builder.finished_data();
    if body.len() > MAX_FRAME {
        bail!("Message exceeds maximum frame size")
    }
    Ok(body.to_vec())
}

pub fn decode(body: &[u8]) -> Result<Message> {
    if body.len() < 8 || body.len() > MAX_FRAME {
        bail!("Invalid FlatBuffers frame length")
    }
    if !flatbuffers::buffer_has_identifier(body, "COGF", false) {
        bail!("Invalid FlatBuffers file identifier")
    }
    let envelope =
        flatbuffers::root::<fb::Envelope>(body).context("Invalid FlatBuffers message")?;
    if envelope.version() != 1 {
        bail!("Unsupported protocol version")
    }
    let id = envelope.request_id().to_string();
    if id.is_empty() || id.len() > 128 {
        bail!("Invalid request correlation ID")
    }
    let fields = [
        envelope.request().is_some(),
        envelope.response().is_some(),
        envelope.error().is_some(),
        envelope.control().is_some(),
        envelope.control_response().is_some(),
    ];
    if fields.into_iter().filter(|present| *present).count() != 1 {
        bail!("Envelope must contain exactly one payload")
    }
    match envelope.kind() {
        fb::MessageKind::Control => {
            let control = envelope.control().context("Missing control request")?;
            let action = match control.action() {
                fb::ControlAction::Status => ControlAction::Status,
                fb::ControlAction::CancelActive => ControlAction::CancelActive,
                fb::ControlAction::Stop => ControlAction::Stop,
                _ => bail!("Unknown control action"),
            };
            Ok(Message::Control { id, action })
        }
        fb::MessageKind::ControlResponse => {
            let response = envelope
                .control_response()
                .context("Missing control response")?;
            Ok(Message::ControlReply {
                id,
                active: response.active(),
                accepted: response.accepted(),
            })
        }
        fb::MessageKind::Request => {
            if envelope.response().is_some() || envelope.error().is_some() {
                bail!("Ambiguous request envelope")
            }
            let req = envelope.request().context("Missing request")?;
            if req.query().trim().is_empty() || req.query().len() > 12_000 {
                bail!("Invalid request query bounds")
            }
            if let Some(reference) = req.conversation_ref() {
                uuid::Uuid::parse_str(reference)?;
            }
            let mode = match req.mode() {
                fb::ReasoningMode::Fast => "fast",
                fb::ReasoningMode::Standard => "standard",
                fb::ReasoningMode::Deep => "deep",
                fb::ReasoningMode::Research => "research",
                _ => bail!("Invalid reasoning mode"),
            };
            Ok(Message::Ask {
                id,
                query: req.query().to_string(),
                mode: mode.to_string(),
                conversation_ref: req.conversation_ref().map(str::to_string),
                explain: req.explain(),
            })
        }
        fb::MessageKind::Response => {
            if envelope.request().is_some() || envelope.error().is_some() {
                bail!("Ambiguous response envelope")
            }
            let r = envelope.response().context("Missing response")?;
            Ok(Message::Reply {
                id,
                outcome: Outcome {
                    task_id: r.task_id().to_string(),
                    answer: r.answer().to_string(),
                    status: r.status().to_string(),
                    calls: r.model_calls(),
                    cycles: r.cycles(),
                    elapsed_ms: r.elapsed_ms(),
                    stop_reason: r.stop_reason().unwrap_or("").to_string(),
                    resources: r
                        .resources()
                        .map(|usage| crate::reasoning::Resources {
                            calls: r.model_calls(),
                            cycles: r.cycles(),
                            reserved_output_tokens: usage.reserved_output_tokens() as usize,
                            prompt_bytes: usage.prompt_bytes() as usize,
                            input_tokens: usage.input_tokens(),
                            output_tokens: usage.output_tokens(),
                            inference_ms: usage.inference_ms(),
                            tool_calls: usage.tool_calls(),
                        })
                        .unwrap_or_default(),
                    evidence: r
                        .evidence()
                        .map(|v| v.iter().map(str::to_string).collect())
                        .unwrap_or_default(),
                    skills: r
                        .skills()
                        .map(|v| v.iter().map(str::to_string).collect())
                        .unwrap_or_default(),
                },
            })
        }
        fb::MessageKind::Error => {
            if envelope.request().is_some() || envelope.response().is_some() {
                bail!("Ambiguous error envelope")
            }
            let err = envelope.error().context("Missing protocol error")?;
            Ok(Message::Error {
                id,
                message: err.message().to_string(),
            })
        }
        _ => bail!("Unknown message kind"),
    }
}

pub async fn write_message<W: AsyncWrite + Unpin>(writer: &mut W, message: &Message) -> Result<()> {
    let bytes = encode(message)?;
    writer
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Message> {
    let len = reader.read_u32().await? as usize;
    if len == 0 || len > MAX_FRAME {
        bail!("Invalid frame length")
    }
    let mut buffer = vec![0u8; len];
    reader.read_exact(&mut buffer).await?;
    decode(&buffer)
}

pub fn new_request(query: String, mode: String) -> Message {
    Message::Ask {
        id: Uuid::new_v4().to_string(),
        query,
        mode,
        conversation_ref: None,
        explain: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_are_typed_and_ambiguous_envelopes_fail() {
        for action in [
            ControlAction::Status,
            ControlAction::CancelActive,
            ControlAction::Stop,
        ] {
            let message = Message::Control {
                id: "control".into(),
                action,
            };
            assert!(
                matches!(decode(&encode(&message).unwrap()).unwrap(), Message::Control { action: decoded, .. } if decoded == action)
            );
        }
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let id = builder.create_string("invalid");
        let control = fb::ControlRequest::create(
            &mut builder,
            &fb::ControlRequestArgs {
                action: fb::ControlAction(255),
            },
        );
        let envelope = fb::Envelope::create(
            &mut builder,
            &fb::EnvelopeArgs {
                request_id: Some(id),
                kind: fb::MessageKind::Control,
                control: Some(control),
                ..Default::default()
            },
        );
        builder.finish(envelope, Some("COGF"));
        assert!(decode(builder.finished_data()).is_err());
        builder.reset();
        let id = builder.create_string("ambiguous");
        let control = fb::ControlRequest::create(&mut builder, &fb::ControlRequestArgs::default());
        let detail = builder.create_string("error");
        let error = fb::ProtocolError::create(
            &mut builder,
            &fb::ProtocolErrorArgs {
                message: Some(detail),
            },
        );
        let envelope = fb::Envelope::create(
            &mut builder,
            &fb::EnvelopeArgs {
                request_id: Some(id),
                kind: fb::MessageKind::Control,
                control: Some(control),
                error: Some(error),
                ..Default::default()
            },
        );
        builder.finish(envelope, Some("COGF"));
        assert!(decode(builder.finished_data()).is_err());
    }
    #[test]
    fn conversation_and_explanation_options_roundtrip() {
        let reference = uuid::Uuid::new_v4().to_string();
        let mut message = new_request("Explain previous result".into(), "standard".into());
        if let Message::Ask {
            conversation_ref,
            explain,
            ..
        } = &mut message
        {
            *conversation_ref = Some(reference.clone());
            *explain = true;
        }
        assert!(
            matches!(decode(&encode(&message).unwrap()).unwrap(),Message::Ask{conversation_ref:Some(id),explain:true,..} if id==reference)
        );
    }

    #[test]
    fn reply_preserves_optional_metrics_and_legacy_v1_defaults() {
        let outcome = Outcome {
            task_id: "task".into(),
            answer: "candidate".into(),
            status: "unverified".into(),
            calls: 2,
            cycles: 1,
            elapsed_ms: 3.0,
            evidence: vec![],
            skills: vec![],
            stop_reason: "unverified".into(),
            resources: crate::reasoning::Resources {
                calls: 2,
                cycles: 1,
                reserved_output_tokens: 512,
                prompt_bytes: 200,
                input_tokens: Some(20),
                output_tokens: None,
                inference_ms: 2.0,
                tool_calls: 1,
            },
        };
        let reply = Message::Reply {
            id: "request".into(),
            outcome,
        };
        let Message::Reply { outcome, .. } = decode(&encode(&reply).unwrap()).unwrap() else {
            panic!("Expected reply")
        };
        assert_eq!(outcome.resources.input_tokens, Some(20));
        assert_eq!(outcome.resources.output_tokens, None);
        assert_eq!(outcome.resources.reserved_output_tokens, 512);
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let id = builder.create_string("request");
        let task = builder.create_string("task");
        let answer = builder.create_string("legacy");
        let status = builder.create_string("completed");
        let response = fb::AskResponse::create(
            &mut builder,
            &fb::AskResponseArgs {
                task_id: Some(task),
                answer: Some(answer),
                status: Some(status),
                ..Default::default()
            },
        );
        let envelope = fb::Envelope::create(
            &mut builder,
            &fb::EnvelopeArgs {
                version: 1,
                request_id: Some(id),
                kind: fb::MessageKind::Response,
                response: Some(response),
                ..Default::default()
            },
        );
        builder.finish(envelope, Some("COGF"));
        let Message::Reply { outcome, .. } = decode(builder.finished_data()).unwrap() else {
            panic!("Expected legacy reply")
        };
        assert_eq!(outcome.answer, "legacy");
        assert_eq!(outcome.resources.input_tokens, None);
        assert_eq!(outcome.stop_reason, "");
    }
    #[test]
    fn typed_round_trip() {
        let message = new_request("calc: 7 * 6".into(), "fast".into());
        let bytes = encode(&message).unwrap();
        assert!(
            matches!(decode(&bytes).unwrap(), Message::Ask { query, .. } if query == "calc: 7 * 6")
        );
    }
    #[test]
    fn reject_garbage() {
        assert!(decode(b"not a valid message").is_err());
    }
    #[test]
    fn reject_oversized() {
        assert!(decode(&vec![0; MAX_FRAME + 1]).is_err());
    }
}

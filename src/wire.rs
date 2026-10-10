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
    Modules,
    TaskProgress,
    CancelTask,
}

#[derive(Clone, Debug)]
pub enum Message {
    Control {
        id: String,
        action: ControlAction,
        task_id: Option<String>,
        after: u64,
    },
    ControlReply {
        id: String,
        active: bool,
        accepted: bool,
        modules: Vec<crate::fabric::ModuleDescriptor>,
        active_task_id: Option<String>,
        progress: Option<(String, crate::events::ProgressPage)>,
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

fn validate_control(action: ControlAction, task: Option<&str>, after: u64) -> Result<()> {
    if matches!(
        action,
        ControlAction::TaskProgress | ControlAction::CancelTask
    ) {
        uuid::Uuid::parse_str(task.context("Task control requires a task UUID")?)?;
    } else if task.is_some() {
        bail!("Unexpected task identity on service control")
    }
    if after > i64::MAX as u64 || (action != ControlAction::TaskProgress && after != 0) {
        bail!("Invalid progress cursor")
    }
    Ok(())
}
fn validate_page(task: &str, page: &crate::events::ProgressPage) -> Result<()> {
    uuid::Uuid::parse_str(task)?;
    if page.events.len() > 32 || page.cursor > i64::MAX as u64 {
        bail!("Invalid progress page bounds")
    }
    let mut last = 0;
    for event in &page.events {
        if event.task_id != task || event.sequence <= last || event.sequence > page.cursor {
            bail!("Progress page task or sequence mismatch")
        }
        last = event.sequence;
    }
    Ok(())
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
        Message::Control {
            action,
            task_id,
            after,
            ..
        } => {
            validate_control(*action, task_id.as_deref(), *after)?;
            let task_id = task_id.as_ref().map(|id| builder.create_string(id));
            args.kind = fb::MessageKind::Control;
            args.control = Some(fb::ControlRequest::create(
                &mut builder,
                &fb::ControlRequestArgs {
                    action: match action {
                        ControlAction::Status => fb::ControlAction::Status,
                        ControlAction::CancelActive => fb::ControlAction::CancelActive,
                        ControlAction::Stop => fb::ControlAction::Stop,
                        ControlAction::Modules => fb::ControlAction::Modules,
                        ControlAction::TaskProgress => fb::ControlAction::TaskProgress,
                        ControlAction::CancelTask => fb::ControlAction::CancelTask,
                    },
                    task_id,
                    after_sequence: *after,
                },
            ));
        }
        Message::ControlReply {
            active,
            accepted,
            modules,
            active_task_id,
            progress,
            ..
        } => {
            if active_task_id.is_some() && !active {
                bail!("Inactive response cannot identify an active task")
            }
            args.kind = fb::MessageKind::ControlResponse;
            if modules.len() > 32 {
                bail!("Too many discovered modules")
            }
            let mut encoded = Vec::new();
            for module in modules {
                module.validate()?;
                let id = builder.create_string(&module.id);
                let version = builder.create_string(&module.version);
                let roles = module
                    .roles
                    .iter()
                    .map(|role| builder.create_string(role))
                    .collect::<Vec<_>>();
                let roles = builder.create_vector(&roles);
                encoded.push(fb::ModuleInfo::create(
                    &mut builder,
                    &fb::ModuleInfoArgs {
                        id: Some(id),
                        version: Some(version),
                        roles: Some(roles),
                    },
                ));
            }
            let modules = builder.create_vector(&encoded);
            let active_task_id = active_task_id
                .as_ref()
                .map(|id| -> Result<_> {
                    uuid::Uuid::parse_str(id)?;
                    Ok(builder.create_string(id))
                })
                .transpose()?;
            let progress = if let Some((task, page)) = progress {
                validate_page(task, page)?;
                let task = builder.create_string(task);
                let mut frames = Vec::new();
                for event in &page.events {
                    let bytes = event.encode()?;
                    let data = builder.create_vector(&bytes);
                    frames.push(fb::ProgressFrame::create(
                        &mut builder,
                        &fb::ProgressFrameArgs { data: Some(data) },
                    ));
                }
                let events = builder.create_vector(&frames);
                Some(fb::ProgressPage::create(
                    &mut builder,
                    &fb::ProgressPageArgs {
                        task_id: Some(task),
                        cursor: page.cursor,
                        more: page.more,
                        events: Some(events),
                    },
                ))
            } else {
                None
            };
            args.control_response = Some(fb::ControlResponse::create(
                &mut builder,
                &fb::ControlResponseArgs {
                    active: *active,
                    accepted: *accepted,
                    modules: Some(modules),
                    active_task_id,
                    progress,
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
                fb::ControlAction::Modules => ControlAction::Modules,
                fb::ControlAction::TaskProgress => ControlAction::TaskProgress,
                fb::ControlAction::CancelTask => ControlAction::CancelTask,
                _ => bail!("Unknown control action"),
            };
            let task_id = control.task_id().map(String::from);
            let after = control.after_sequence();
            validate_control(action, task_id.as_deref(), after)?;
            Ok(Message::Control {
                id,
                action,
                task_id,
                after,
            })
        }
        fb::MessageKind::ControlResponse => {
            let response = envelope
                .control_response()
                .context("Missing control response")?;
            let mut modules = Vec::new();
            if let Some(entries) = response.modules() {
                if entries.len() > 32 {
                    bail!("Too many discovered modules")
                }
                for entry in entries.iter() {
                    let descriptor = crate::fabric::ModuleDescriptor {
                        id: entry.id().into(),
                        version: entry.version().into(),
                        roles: entry
                            .roles()
                            .map(|roles| roles.iter().map(str::to_string).collect())
                            .unwrap_or_default(),
                    };
                    descriptor.validate()?;
                    modules.push(descriptor);
                }
            }
            let active_task_id = response.active_task_id().map(String::from);
            if active_task_id.is_some() && !response.active() {
                bail!("Inactive response cannot identify an active task")
            }
            if let Some(id) = &active_task_id {
                uuid::Uuid::parse_str(id)?;
            }
            let progress = if let Some(page) = response.progress() {
                let mut events = Vec::new();
                if let Some(frames) = page.events() {
                    if frames.len() > 32 {
                        bail!("Progress page exceeds event limit")
                    }
                    for frame in frames.iter() {
                        events.push(crate::events::Progress::decode(frame.data().bytes())?);
                    }
                }
                let task = page.task_id().to_string();
                let page = crate::events::ProgressPage {
                    cursor: page.cursor(),
                    more: page.more(),
                    events,
                };
                validate_page(&task, &page)?;
                Some((task, page))
            } else {
                None
            };
            Ok(Message::ControlReply {
                id,
                active: response.active(),
                accepted: response.accepted(),
                modules,
                active_task_id,
                progress,
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
    fn targeted_controls_require_identity_and_preserve_progress_frames() {
        let task = Uuid::new_v4().to_string();
        for action in [ControlAction::TaskProgress, ControlAction::CancelTask] {
            let message = Message::Control {
                id: "control".into(),
                action,
                task_id: Some(task.clone()),
                after: 0,
            };
            assert!(
                matches!(decode(&encode(&message).unwrap()).unwrap(),Message::Control{task_id:Some(identity),..} if identity==task)
            );
            assert!(encode(&Message::Control {
                id: "control".into(),
                action,
                task_id: None,
                after: 0
            })
            .is_err());
        }
        assert!(encode(&Message::Control {
            id: "c".into(),
            action: ControlAction::Status,
            task_id: Some(task.clone()),
            after: 0
        })
        .is_err());
        assert!(encode(&Message::Control {
            id: "c".into(),
            action: ControlAction::CancelTask,
            task_id: Some(task.clone()),
            after: 1
        })
        .is_err());
        let event = crate::events::Progress::from_audit(
            7,
            &task,
            "model_dispatch",
            "jev-r",
            &serde_json::json!({"role":"solver","call":1}),
        )
        .unwrap()
        .unwrap();
        let reply = Message::ControlReply {
            id: "c".into(),
            active: true,
            accepted: true,
            modules: Vec::new(),
            active_task_id: Some(task.clone()),
            progress: Some((
                task.clone(),
                crate::events::ProgressPage {
                    cursor: 8,
                    more: false,
                    events: vec![event.clone()],
                },
            )),
        };
        let Message::ControlReply {
            progress: Some((identity, page)),
            ..
        } = decode(&encode(&reply).unwrap()).unwrap()
        else {
            panic!("Expected progress")
        };
        assert_eq!(identity, task);
        assert_eq!(page.cursor, 8);
        assert_eq!(page.events[0].model_calls, Some(1));
        let mut invalid = page.clone();
        invalid.cursor = 6;
        assert!(validate_page(&task, &invalid).is_err());
        invalid = page;
        invalid.events.push(event);
        assert!(validate_page(&task, &invalid).is_err());
        assert!(validate_page(&Uuid::new_v4().to_string(), &invalid).is_err());
    }
    #[test]
    fn nested_progress_frames_are_verified_before_access() {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let bytes = builder.create_vector(b"garbage");
        let frame =
            fb::ProgressFrame::create(&mut builder, &fb::ProgressFrameArgs { data: Some(bytes) });
        let events = builder.create_vector(&[frame]);
        let task = builder.create_string(&Uuid::new_v4().to_string());
        let page = fb::ProgressPage::create(
            &mut builder,
            &fb::ProgressPageArgs {
                task_id: Some(task),
                cursor: 1,
                more: false,
                events: Some(events),
            },
        );
        let response = fb::ControlResponse::create(
            &mut builder,
            &fb::ControlResponseArgs {
                accepted: true,
                progress: Some(page),
                ..Default::default()
            },
        );
        let id = builder.create_string("control");
        let envelope = fb::Envelope::create(
            &mut builder,
            &fb::EnvelopeArgs {
                request_id: Some(id),
                kind: fb::MessageKind::ControlResponse,
                control_response: Some(response),
                ..Default::default()
            },
        );
        builder.finish(envelope, Some("COGF"));
        assert!(decode(builder.finished_data()).is_err());
    }

    #[test]
    fn controls_are_typed_and_ambiguous_envelopes_fail() {
        for action in [
            ControlAction::Status,
            ControlAction::CancelActive,
            ControlAction::Stop,
            ControlAction::Modules,
        ] {
            let message = Message::Control {
                id: "control".into(),
                action,
                task_id: None,
                after: 0,
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
                ..Default::default()
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
    fn discovery_roundtrips_and_legacy_controls_have_no_modules() {
        let reply = Message::ControlReply {
            active_task_id: None,
            progress: None,
            id: "discovery".into(),
            active: false,
            accepted: true,
            modules: vec![crate::fabric::ModuleDescriptor {
                id: "inference.local".into(),
                version: "1.0.0".into(),
                roles: vec!["solver".into()],
            }],
        };
        let Message::ControlReply { modules, .. } = decode(&encode(&reply).unwrap()).unwrap()
        else {
            panic!("Expected control response")
        };
        assert_eq!(modules[0].id, "inference.local");
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let id = builder.create_string("legacy");
        let response = fb::ControlResponse::create(
            &mut builder,
            &fb::ControlResponseArgs {
                active: false,
                accepted: true,
                ..Default::default()
            },
        );
        let envelope = fb::Envelope::create(
            &mut builder,
            &fb::EnvelopeArgs {
                request_id: Some(id),
                kind: fb::MessageKind::ControlResponse,
                control_response: Some(response),
                ..Default::default()
            },
        );
        builder.finish(envelope, Some("COGF"));
        assert!(
            matches!(decode(builder.finished_data()).unwrap(),Message::ControlReply { modules, active_task_id:None, progress:None, .. } if modules.is_empty())
        );
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

//! Verified bounded FlatBuffers module packets.
use crate::backend::Generation;
use anyhow::{bail, Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/module_generated.rs"));
}
use generated::fabric as fb;

pub(super) const MAX_FRAME: usize = 256 * 1024;
pub(crate) const ROLES: [&str; 7] = [
    "interface",
    "planner",
    "solver",
    "explorer",
    "critic",
    "reflector",
    "synthesizer",
];

#[derive(Clone)]
pub(super) struct Header {
    pub(super) correlation: String,
    pub(super) task: String,
    pub(super) sender: String,
    pub(super) destination: String,
    pub(super) module_version: String,
    pub(super) expires: u64,
}
pub(super) enum Payload {
    Request {
        role: String,
        prompt: String,
        tokens: usize,
    },
    Response(Generation),
    Error(String),
}
pub(super) struct Packet {
    pub(super) header: Header,
    pub(super) payload: Payload,
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

pub(crate) fn actor(role: &str) -> Result<&'static str> {
    match role {
        "interface" | "synthesizer" => Ok("interface"),
        "planner" | "solver" | "explorer" | "critic" | "reflector" => Ok("jev-r"),
        _ => bail!("Unknown module inference role"),
    }
}

pub(super) fn encode(packet: &Packet) -> Result<Vec<u8>> {
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let header = &packet.header;
    let mut args = fb::PacketArgs {
        version: 1,
        correlation_id: Some(builder.create_string(&header.correlation)),
        task_id: Some(builder.create_string(&header.task)),
        sender: Some(builder.create_string(&header.sender)),
        destination: Some(builder.create_string(&header.destination)),
        module_version: Some(builder.create_string(&header.module_version)),
        expires_at_ms: header.expires,
        ..Default::default()
    };
    match &packet.payload {
        Payload::Request {
            role,
            prompt,
            tokens,
        } => {
            let index = ROLES
                .iter()
                .position(|candidate| candidate == role)
                .context("Unknown role")?;
            let prompt = builder.create_string(prompt);
            args.request = Some(fb::InferenceRequest::create(
                &mut builder,
                &fb::InferenceRequestArgs {
                    role: fb::Role(index as u8),
                    prompt: Some(prompt),
                    max_tokens: u32::try_from(*tokens)?,
                },
            ));
        }
        Payload::Response(report) => {
            let text = builder.create_string(&report.text);
            let model = builder.create_string(&report.model);
            args.response = Some(fb::InferenceResponse::create(
                &mut builder,
                &fb::InferenceResponseArgs {
                    text: Some(text),
                    model: Some(model),
                    input_tokens: report.input_tokens,
                    output_tokens: report.output_tokens,
                    elapsed_ms: report.elapsed_ms,
                    queue_wait_ms: report.queue_wait_ms,
                },
            ));
        }
        Payload::Error(message) => args.error = Some(builder.create_string(message)),
    }
    let root = fb::Packet::create(&mut builder, &args);
    builder.finish(root, Some("COGM"));
    let bytes = builder.finished_data();
    if bytes.len() > MAX_FRAME {
        bail!("Module frame exceeds bounds")
    }
    Ok(bytes.to_vec())
}

pub(super) fn decode(bytes: &[u8]) -> Result<Packet> {
    if bytes.len() < 8
        || bytes.len() > MAX_FRAME
        || !flatbuffers::buffer_has_identifier(bytes, "COGM", false)
    {
        bail!("Invalid module frame")
    }
    let packet = flatbuffers::root::<fb::Packet>(bytes).context("Malformed module FlatBuffer")?;
    if packet.version() != 1 {
        bail!("Unsupported module protocol version")
    }
    uuid::Uuid::parse_str(packet.task_id())?;
    uuid::Uuid::parse_str(packet.correlation_id())?;
    if packet.sender().is_empty()
        || packet.sender().len() > 64
        || packet.destination().is_empty()
        || packet.destination().len() > 64
        || packet.module_version().len() > 128
    {
        bail!("Invalid module identity bounds")
    }
    semver::Version::parse(packet.module_version())?;
    if [
        packet.request().is_some(),
        packet.response().is_some(),
        packet.error().is_some(),
    ]
    .into_iter()
    .filter(|value| *value)
    .count()
        != 1
    {
        bail!("Ambiguous module packet")
    }
    let payload = if let Some(request) = packet.request() {
        let role = ROLES
            .get(request.role().0 as usize)
            .context("Unknown module role")?;
        if request.prompt().len() > 32_000
            || request.max_tokens() == 0
            || request.max_tokens() > 16_384
        {
            bail!("Invalid module request bounds")
        }
        Payload::Request {
            role: role.to_string(),
            prompt: request.prompt().into(),
            tokens: request.max_tokens() as usize,
        }
    } else if let Some(response) = packet.response() {
        if response.text().trim().is_empty()
            || response.text().len() > 96_000
            || response.model().is_empty()
            || response.model().len() > 256
            || !response.elapsed_ms().is_finite()
            || response.elapsed_ms() < 0.0
            || response
                .queue_wait_ms()
                .is_some_and(|ms| !ms.is_finite() || !(0.0..=300_000.0).contains(&ms))
        {
            bail!("Invalid module response bounds")
        }
        Payload::Response(Generation {
            text: response.text().into(),
            model: response.model().into(),
            input_tokens: response.input_tokens(),
            output_tokens: response.output_tokens(),
            elapsed_ms: response.elapsed_ms(),
            queue_wait_ms: response.queue_wait_ms(),
        })
    } else {
        let error = packet.error().context("Missing module payload")?;
        if error.len() > 1000 {
            bail!("Oversized module error")
        }
        Payload::Error(error.into())
    };
    Ok(Packet {
        header: Header {
            correlation: packet.correlation_id().into(),
            task: packet.task_id().into(),
            sender: packet.sender().into(),
            destination: packet.destination().into(),
            module_version: packet.module_version().into(),
            expires: packet.expires_at_ms(),
        },
        payload,
    })
}

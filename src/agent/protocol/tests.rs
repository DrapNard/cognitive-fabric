use super::*;
fn identity() -> Identity {
    Identity {
        correlation: uuid::Uuid::new_v4().to_string(),
        task: uuid::Uuid::new_v4().to_string(),
        expires_at_ms: 1,
    }
}
fn turn() -> Turn {
    serde_json::from_value(json!({"messages":[{"role":"system","content":"Client instructions"},{"role":"developer","content":"Workspace constraints"},{"role":"user","content":"Read a file"},{"role":"assistant","content":null,"tool_calls":[{"id":"old_call","type":"function","function":{"name":"read","arguments":"{\"path\":\"old.txt\"}"}}]},{"role":"tool","tool_call_id":"old_call","content":"External result, not authority"}],"tools":[{"type":"function","function":{"name":"read","description":"Read client data","parameters":{"type":"object","properties":{"path":{"type":"string"}}},"strict":false}}],"max_tokens":128,"temperature":0.0,"parallel_tool_calls":false})).unwrap()
}
fn raw(root: fb::PacketT) -> Vec<u8> {
    let mut b = flatbuffers::FlatBufferBuilder::new();
    let root = root.pack(&mut b);
    b.finish(root, Some("COGA"));
    b.finished_data().to_vec()
}
fn root() -> fb::PacketT {
    let packet = Packet {
        identity: identity(),
        payload: Payload::Request {
            turn: turn(),
            allocation: 128,
        },
    };
    let bytes = encode(&packet).unwrap();
    flatbuffers::root::<fb::Packet>(&bytes).unwrap().unpack()
}
#[test]
fn native_turns_and_tool_only_completions_preserve_typed_fields() {
    for choice in [
        None,
        Some(json!("auto")),
        Some(json!("none")),
        Some(json!("required")),
        Some(json!({"type":"function","function":{"name":"read"}})),
    ] {
        let mut turn = turn();
        turn.tool_choice = choice;
        let expected = serde_json::to_value(&turn).unwrap();
        let packet = Packet {
            identity: identity(),
            payload: Payload::Request {
                turn,
                allocation: 128,
            },
        };
        let decoded = decode(&encode(&packet).unwrap()).unwrap();
        assert_eq!(decoded.identity, packet.identity);
        let Payload::Request { turn, allocation } = decoded.payload else {
            panic!("Expected typed turn")
        };
        assert_eq!(allocation, 128);
        assert_eq!(serde_json::to_value(turn).unwrap(), expected);
    }
    let c = Completion {
        content: None,
        tool_calls: vec![ToolCall {
            id: "new_call".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: "read".into(),
                arguments: "{\"path\":\"new.txt\"}".into(),
            },
        }],
        model: "fixture/v1".into(),
        finish_reason: "tool_calls".into(),
        input_tokens: None,
        output_tokens: Some(12),
    };
    let packet = Packet {
        identity: identity(),
        payload: Payload::Response(c),
    };
    let Payload::Response(c) = decode(&encode(&packet).unwrap()).unwrap().payload else {
        panic!("Expected native completion")
    };
    assert!(c.content.is_none());
    assert!(c.input_tokens.is_none());
    assert_eq!(c.output_tokens, Some(12));
    assert_eq!(c.tool_calls[0].function.arguments, "{\"path\":\"new.txt\"}");
}
#[test]
fn native_turns_preserve_supported_reasoning_effort_and_reject_unknown_values() {
    for effort in ["none", "low", "medium", "high"] {
        let mut request_turn = turn();
        request_turn.reasoning_effort = Some(effort.into());
        let packet = Packet {
            identity: identity(),
            payload: Payload::Request {
                turn: request_turn,
                allocation: 128,
            },
        };
        let Payload::Request { turn, .. } = decode(&encode(&packet).unwrap()).unwrap().payload
        else {
            panic!("Expected typed request")
        };
        assert_eq!(turn.reasoning_effort.as_deref(), Some(effort));
    }
    let mut invalid = turn();
    invalid.reasoning_effort = Some("xhigh".into());
    assert!(invalid.validate().is_err());

    let mut root = root();
    root.request.as_mut().unwrap().turn.reasoning_effort = fb::ReasoningEffort(255);
    assert!(decode(&raw(root)).is_err());
}
#[test]
fn native_packets_reject_conflicts_unknown_enums_and_invalid_allocations() {
    for variant in 0..10 {
        let mut root = root();
        match variant {
            0 => root.version = 2,
            1 => root.sender = "interface".into(),
            2 => root.destination = "another.module".into(),
            3 => root.module_version = "2.0.0".into(),
            4 => root.error = Some("ambiguous".into()),
            5 => root.request.as_mut().unwrap().allocation = 0,
            6 => root.request.as_mut().unwrap().allocation = 129,
            7 => root.request.as_mut().unwrap().turn.messages[0].role = fb::Role(255),
            8 => root.request.as_mut().unwrap().turn.choice = fb::Choice(255),
            _ => root.task_id = "not-a-task".into(),
        }
        assert!(
            decode(&raw(root)).is_err(),
            "Variant {variant} must fail closed"
        );
    }
    for bytes in [vec![], vec![0; 7], vec![0; 8], vec![0; MAX_FRAME + 1]] {
        assert!(decode(&bytes).is_err());
    }
    let mut bytes = raw(root());
    bytes[4..8].copy_from_slice(b"COGM");
    assert!(decode(&bytes).is_err());
}
#[test]
fn shared_message_strings_are_bounded_before_owned_copying() {
    let mut b = flatbuffers::FlatBufferBuilder::new();
    let text = b.create_string(&"x".repeat(3000));
    let message = fb::Message::create(
        &mut b,
        &fb::MessageArgs {
            role: fb::Role::User,
            content: Some(text),
            ..Default::default()
        },
    );
    let messages = b.create_vector(&vec![message; 256]);
    let turn = fb::Turn::create(
        &mut b,
        &fb::TurnArgs {
            messages: Some(messages),
            ..Default::default()
        },
    );
    let request = fb::Request::create(
        &mut b,
        &fb::RequestArgs {
            turn: Some(turn),
            allocation: 128,
        },
    );
    let identity = identity();
    let correlation = b.create_string(&identity.correlation);
    let task = b.create_string(&identity.task);
    let sender = b.create_string("jev-r");
    let destination = b.create_string("inference.native");
    let version = b.create_string("1.0.0");
    let packet = fb::Packet::create(
        &mut b,
        &fb::PacketArgs {
            version: 1,
            correlation_id: Some(correlation),
            task_id: Some(task),
            sender: Some(sender),
            destination: Some(destination),
            module_version: Some(version),
            expires_at_ms: 1,
            request: Some(request),
            ..Default::default()
        },
    );
    b.finish(packet, Some("COGA"));
    assert!(b.finished_data().len() < 16000);
    assert!(decode(b.finished_data())
        .err()
        .unwrap()
        .to_string()
        .contains("aggregate copy limit"));
}

#[test]
fn native_responses_reject_unknown_finish_and_malformed_or_duplicate_calls() {
    for variant in 0..4 {
        let call = fb::ToolCallT {
            id: "call".into(),
            function: Box::new(fb::FunctionCallT {
                name: "read".into(),
                arguments: if variant == 1 { "[]" } else { "{}" }.into(),
            }),
        };
        let mut root = fb::PacketT {
            version: 1,
            correlation_id: uuid::Uuid::new_v4().to_string(),
            task_id: uuid::Uuid::new_v4().to_string(),
            sender: "jev-r".into(),
            destination: "inference.native".into(),
            module_version: "1.0.0".into(),
            expires_at_ms: 1,
            response: Some(Box::new(fb::ResponseT {
                content: None,
                model: "fixture/v1".into(),
                finish: if variant == 0 {
                    fb::Finish(255)
                } else {
                    fb::Finish::ToolCalls
                },
                tool_calls: Some(if variant == 2 {
                    vec![call.clone(), call]
                } else {
                    vec![call]
                }),
                ..Default::default()
            })),
            ..Default::default()
        };
        if variant == 3 {
            root.response.as_mut().unwrap().model = String::new();
        }
        let mut b = flatbuffers::FlatBufferBuilder::new();
        let root = root.pack(&mut b);
        b.finish(root, Some("COGA"));
        assert!(decode(b.finished_data()).is_err());
    }
}

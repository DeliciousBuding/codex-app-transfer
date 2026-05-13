use std::path::PathBuf;

use serde_json::Value;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("anthropic_messages")
}

fn read_fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_root().join(name)).expect("fixture should be readable")
}

fn read_json_fixture(name: &str) -> Value {
    serde_json::from_str(&read_fixture(name)).expect("fixture should be valid json")
}

#[derive(Debug)]
struct SseFrame {
    event: String,
    data: Value,
}

fn parse_sse_fixture(name: &str) -> Vec<SseFrame> {
    read_fixture(name)
        .split("\n\n")
        .filter(|frame| !frame.trim().is_empty())
        .map(|frame| {
            let mut event = None;
            let mut data = None;
            for line in frame.lines() {
                if let Some(value) = line.strip_prefix("event: ") {
                    event = Some(value.to_owned());
                } else if let Some(value) = line.strip_prefix("data: ") {
                    data = Some(value.to_owned());
                }
            }
            SseFrame {
                event: event.expect("SSE frame should include event"),
                data: serde_json::from_str(&data.expect("SSE frame should include data"))
                    .expect("SSE data should be valid json"),
            }
        })
        .collect()
}

#[test]
fn p2_anthropic_messages_sse_fixtures_are_valid() {
    let cases = [
        (
            "text_stream.sse",
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ],
        ),
        (
            "thinking_stream.sse",
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ],
        ),
        (
            "tool_use_stream.sse",
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ],
        ),
        ("error_stream.sse", vec!["error"]),
        (
            "unknown_event_stream.sse",
            vec![
                "message_start",
                "anthropic_future_event",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ],
        ),
    ];

    for (fixture, expected_events) in cases {
        let frames = parse_sse_fixture(fixture);
        let events: Vec<_> = frames.iter().map(|frame| frame.event.as_str()).collect();
        assert_eq!(
            events, expected_events,
            "unexpected event order in {fixture}"
        );
        for frame in frames {
            assert_eq!(
                frame.data["type"].as_str(),
                Some(frame.event.as_str()),
                "fixture {fixture} should keep event name and data.type aligned"
            );
        }
    }
}

#[test]
fn p2_request_mapper_json_fixtures_are_valid() {
    let cases = [
        ("request_text.responses.json", "request_text.anthropic.json"),
        (
            "request_tool_result.responses.json",
            "request_tool_result.anthropic.json",
        ),
    ];

    for (input_name, expected_name) in cases {
        let input = read_json_fixture(input_name);
        let expected = read_json_fixture(expected_name);
        assert!(
            input.get("input").is_some(),
            "{input_name} should model Responses input"
        );
        assert!(
            expected.get("messages").is_some(),
            "{expected_name} should model Anthropic Messages output"
        );
        assert!(
            expected.get("max_tokens").is_some(),
            "{expected_name} should include Anthropic required max_tokens"
        );
    }
}

#[test]
#[ignore = "P3 replaces the placeholder with AnthropicMessages request mapper"]
fn responses_text_request_lowers_to_anthropic_messages() {
    let input = read_json_fixture("request_text.responses.json");
    let expected = read_json_fixture("request_text.anthropic.json");

    let actual = p3_anthropic_messages_mapper_not_implemented(input);

    assert_eq!(actual, expected);
}

#[test]
#[ignore = "P3 replaces the placeholder with AnthropicMessages request mapper"]
fn responses_tool_result_request_lowers_to_anthropic_messages() {
    let input = read_json_fixture("request_tool_result.responses.json");
    let expected = read_json_fixture("request_tool_result.anthropic.json");

    let actual = p3_anthropic_messages_mapper_not_implemented(input);

    assert_eq!(actual, expected);
}

fn p3_anthropic_messages_mapper_not_implemented(_input: Value) -> Value {
    panic!("P3 must call the real Anthropic Messages request mapper here");
}

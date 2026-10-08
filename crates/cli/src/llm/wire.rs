//! Conversations in the wire formats of chat providers.
//!
//! Anthropic's Messages API and OpenAI-compatible chat completions take the whole conversation
//! with every request. A [`Transcript`] keeps it in the provider's format: the prompts, and each
//! reply as the provider sent it, so reasoning the provider returns goes back with the reply it
//! led to. Messages requests mark the brief and the newest prompt as prompt-cache breakpoints, so
//! a turn reads the conversation so far from Anthropic's cache instead of paying for it in full.
//!
//! Replies stream as server-sent events. An [`SseParser`] splits the bytes into events wherever
//! the chunks break, and a [`ReplyStream`] turns the events into the reasoning and text they add
//! while it assembles the reply in the shape it would have arrived unstreamed, so the transcript
//! reads streamed and whole replies alike.

use super::provider::Usage;
use serde_json::{Map, Value, json};
use solar_config::LlmEffort;

/// The Messages API version the requests follow.
pub(super) const ANTHROPIC_VERSION: &str = "2023-06-01";
/// The most tokens a reply may use, reasoning included.
const MAX_OUTPUT_TOKENS: u64 = 32_000;

/// The wire format of a chat provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Protocol {
    /// Anthropic's Messages API.
    Messages,
    /// OpenAI-compatible chat completions.
    ChatCompletions,
}

impl Protocol {
    /// The endpoint's path under the API base URL.
    pub(super) const fn path(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::ChatCompletions => "chat/completions",
        }
    }
}

/// One conversation in a provider's wire format.
#[derive(Clone, Debug)]
pub(super) struct Transcript {
    protocol: Protocol,
    model: String,
    effort: Option<LlmEffort>,
    messages: Vec<Value>,
}

impl Transcript {
    pub(super) fn new(protocol: Protocol, model: String, effort: Option<LlmEffort>) -> Self {
        Self { protocol, model, effort, messages: Vec::new() }
    }

    /// Adds a prompt.
    pub(super) fn push_prompt(&mut self, text: &str) {
        self.messages.push(match self.protocol {
            Protocol::Messages => {
                json!({"role": "user", "content": [{"type": "text", "text": text}]})
            }
            Protocol::ChatCompletions => json!({"role": "user", "content": text}),
        });
    }

    /// The request for a reply to the conversation so far, which `instructions` open.
    pub(super) fn request(&self, instructions: &str) -> Value {
        match self.protocol {
            Protocol::Messages => {
                // Only the newest prompt is a breakpoint, since a request may carry four.
                let mut messages = self.messages.clone();
                if let Some(block) = messages
                    .last_mut()
                    .and_then(|message| message["content"].as_array_mut())
                    .and_then(|content| content.last_mut())
                {
                    block["cache_control"] = json!({"type": "ephemeral"});
                }
                let mut body = json!({
                    "model": self.model,
                    "max_tokens": MAX_OUTPUT_TOKENS,
                    "stream": true,
                    "system": [{
                        "type": "text",
                        "text": instructions,
                        "cache_control": {"type": "ephemeral"},
                    }],
                    "messages": messages,
                });
                match self.effort {
                    None => {}
                    Some(LlmEffort::None) => body["thinking"] = json!({"type": "disabled"}),
                    Some(effort) => {
                        body["thinking"] = json!({"type": "adaptive"});
                        body["output_config"] = json!({"effort": effort.to_str()});
                    }
                }
                body
            }
            Protocol::ChatCompletions => {
                let mut messages = Vec::with_capacity(self.messages.len() + 1);
                messages.push(json!({"role": "system", "content": instructions}));
                messages.extend(self.messages.iter().cloned());
                let mut body = json!({
                    "model": self.model,
                    "max_tokens": MAX_OUTPUT_TOKENS,
                    "messages": messages,
                    "stream": true,
                    "stream_options": {"include_usage": true},
                });
                if let Some(effort) = self.effort {
                    body["reasoning_effort"] = json!(effort.to_str());
                }
                body
            }
        }
    }

    /// Adds `reply` to the conversation, returning its text and the tokens it used.
    pub(super) fn push_reply(&mut self, reply: &Value) -> Result<(String, Usage), String> {
        let usage = &reply["usage"];
        let tokens = |field: &str| usage[field].as_u64().unwrap_or(0);
        match self.protocol {
            Protocol::Messages => {
                let content = reply["content"].as_array().ok_or("the reply holds no content")?;
                if reply["stop_reason"] == "refusal" {
                    return Err("the model declined to answer".into());
                }
                let text = content
                    .iter()
                    .filter(|block| block["type"] == "text")
                    .filter_map(|block| block["text"].as_str())
                    .collect::<String>();
                // The reply goes back as sent, reasoning blocks and their signatures included.
                self.messages.push(json!({"role": "assistant", "content": content}));
                let usage = Usage {
                    input: tokens("input_tokens"),
                    output: tokens("output_tokens"),
                    cache_read: tokens("cache_read_input_tokens"),
                    cache_write: tokens("cache_creation_input_tokens"),
                };
                Ok((text, usage))
            }
            Protocol::ChatCompletions => {
                let message = &reply["choices"][0]["message"];
                if !message.is_object() {
                    return Err("the reply holds no message".into());
                }
                let text = message["content"].as_str().unwrap_or_default().to_string();
                let mut sent = json!({"role": "assistant", "content": text});
                // A reasoning model expects its reasoning back with the reply it led to.
                if let Some(reasoning) = message["reasoning_content"].as_str() {
                    sent["reasoning_content"] = json!(reasoning);
                }
                self.messages.push(sent);
                // Cache hits sit in the prompt details for OpenAI and at the top for DeepSeek.
                let prompt = tokens("prompt_tokens");
                let cached = usage["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or_else(|| tokens("prompt_cache_hit_tokens"))
                    .min(prompt);
                let usage = Usage {
                    input: prompt - cached,
                    output: tokens("completion_tokens"),
                    cache_read: cached,
                    cache_write: 0,
                };
                Ok((text, usage))
            }
        }
    }

    /// Starts reading a streamed reply to the conversation.
    pub(super) fn reply_stream(&self) -> ReplyStream {
        ReplyStream {
            protocol: self.protocol,
            content: Vec::new(),
            text: String::new(),
            reasoning: None,
            stop: Value::Null,
            usage: Map::new(),
            ended: false,
        }
    }
}

/// A piece of a reply as it streams in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Delta {
    /// Reasoning the model shows while it works.
    Reasoning(String),
    /// Text of the reply.
    Reply(String),
}

/// One server-sent event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SseEvent {
    /// The event's name, when it has one.
    pub(super) name: Option<String>,
    /// The event's data, its lines joined by newlines.
    pub(super) data: String,
}

/// Splits a server-sent event stream into events, wherever its chunks break.
#[derive(Debug, Default)]
pub(super) struct SseParser {
    /// Bytes of a line not yet ended.
    line: Vec<u8>,
    /// The event being read.
    event: SseEvent,
}

impl SseParser {
    /// Reads `bytes`, returning the events they complete.
    pub(super) fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        let mut events = Vec::new();
        for &byte in bytes {
            if byte != b'\n' {
                self.line.push(byte);
                continue;
            }
            let line = std::mem::take(&mut self.line);
            let line = String::from_utf8_lossy(&line);
            let line = line.strip_suffix('\r').unwrap_or(&line);
            if line.is_empty() {
                // A blank line ends the event.
                if self.event != SseEvent::default() {
                    events.push(std::mem::take(&mut self.event));
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                if !self.event.data.is_empty() {
                    self.event.data.push('\n');
                }
                self.event.data.push_str(data.strip_prefix(' ').unwrap_or(data));
            } else if let Some(name) = line.strip_prefix("event:") {
                self.event.name = Some(name.trim().to_string());
            }
        }
        events
    }

    /// Returns the event a stream that stops without a blank line leaves unfinished.
    pub(super) fn finish(mut self) -> Option<SseEvent> {
        self.push(b"\n\n").pop()
    }
}

/// A reply assembled from its streamed events.
#[derive(Debug)]
pub(super) struct ReplyStream {
    protocol: Protocol,
    /// Anthropic's content blocks.
    content: Vec<Value>,
    /// A chat completion's text.
    text: String,
    /// A chat completion's reasoning, when the model shows any.
    reasoning: Option<String>,
    /// Why the reply stopped.
    stop: Value,
    /// The tokens it used, as last reported.
    usage: Map<String, Value>,
    /// Whether the provider marked the end of the reply.
    ended: bool,
}

impl ReplyStream {
    /// Reads one event, returning the reasoning and text it adds.
    pub(super) fn read(&mut self, event: &SseEvent) -> Result<Vec<Delta>, String> {
        let mut deltas = Vec::new();
        if self.protocol == Protocol::ChatCompletions && event.data.trim() == "[DONE]" {
            self.ended = true;
            return Ok(deltas);
        }
        let data = serde_json::from_str::<Value>(&event.data)
            .map_err(|error| format!("the reply streamed an unreadable event: {error}"))?;
        if data["type"] == "error" || data.get("error").is_some_and(|error| !error.is_null()) {
            let message = data["error"]["message"].as_str().unwrap_or("the reply failed");
            return Err(message.to_string());
        }
        match self.protocol {
            Protocol::Messages => match data["type"].as_str().unwrap_or_default() {
                "message_start" => self.add_usage(&data["message"]["usage"]),
                "content_block_start" => {
                    // Blocks start in order, each an object: a block far ahead would make the
                    // reply allocate every block before it, and deltas write into objects.
                    let index = block_index(&data)?;
                    if index != self.content.len() {
                        return Err(format!(
                            "the reply started block {index} where block {} was due",
                            self.content.len()
                        ));
                    }
                    let block = &data["content_block"];
                    if !block.is_object() {
                        return Err("the reply started a block that is not an object".into());
                    }
                    self.content.push(block.clone());
                }
                "content_block_delta" => {
                    let block = self
                        .content
                        .get_mut(block_index(&data)?)
                        .ok_or("the reply streamed into a block it had not started")?;
                    let delta = &data["delta"];
                    let (field, shown) = match delta["type"].as_str().unwrap_or_default() {
                        "text_delta" => ("text", Some(Delta::Reply as fn(String) -> Delta)),
                        "thinking_delta" => ("thinking", Some(Delta::Reasoning as _)),
                        "signature_delta" => ("signature", None),
                        _ => return Ok(deltas),
                    };
                    let piece = delta[field].as_str().unwrap_or_default();
                    block[field] = json!(format!("{}{piece}", block[field].as_str().unwrap_or("")));
                    if let Some(shown) = shown
                        && !piece.is_empty()
                    {
                        deltas.push(shown(piece.to_string()));
                    }
                }
                "message_delta" => {
                    self.stop = data["delta"]["stop_reason"].clone();
                    self.add_usage(&data["usage"]);
                }
                "message_stop" => self.ended = true,
                _ => {}
            },
            Protocol::ChatCompletions => {
                self.add_usage(&data["usage"]);
                if let Some(choice) = data["choices"].get(0) {
                    let delta = &choice["delta"];
                    if let Some(piece) = delta["reasoning_content"].as_str()
                        && !piece.is_empty()
                    {
                        self.reasoning.get_or_insert_default().push_str(piece);
                        deltas.push(Delta::Reasoning(piece.to_string()));
                    }
                    if let Some(piece) = delta["content"].as_str()
                        && !piece.is_empty()
                    {
                        self.text.push_str(piece);
                        deltas.push(Delta::Reply(piece.to_string()));
                    }
                    if !choice["finish_reason"].is_null() {
                        self.stop = choice["finish_reason"].clone();
                    }
                }
            }
        }
        Ok(deltas)
    }

    /// The whole reply, in the shape it would have arrived unstreamed.
    pub(super) fn finish(self) -> Result<Value, String> {
        if !self.ended && (self.protocol == Protocol::Messages || self.stop.is_null()) {
            return Err("the reply broke off".into());
        }
        Ok(match self.protocol {
            Protocol::Messages => {
                json!({"content": self.content, "stop_reason": self.stop, "usage": self.usage})
            }
            Protocol::ChatCompletions => {
                let mut message = json!({"role": "assistant", "content": self.text});
                if let Some(reasoning) = self.reasoning {
                    message["reasoning_content"] = json!(reasoning);
                }
                json!({"choices": [{"message": message, "finish_reason": self.stop}], "usage": self.usage})
            }
        })
    }

    /// The reasoning and text of an unstreamed reply, for a server that did not stream it.
    pub(super) fn deltas_of(&self, reply: &Value) -> Vec<Delta> {
        let mut deltas = Vec::new();
        let mut show = |text: Option<&str>, delta: fn(String) -> Delta| {
            if let Some(text) = text.filter(|text| !text.is_empty()) {
                deltas.push(delta(text.to_string()));
            }
        };
        match self.protocol {
            Protocol::Messages => {
                for block in reply["content"].as_array().into_iter().flatten() {
                    show(block["thinking"].as_str(), Delta::Reasoning);
                    show(block["text"].as_str(), Delta::Reply);
                }
            }
            Protocol::ChatCompletions => {
                let message = &reply["choices"][0]["message"];
                show(message["reasoning_content"].as_str(), Delta::Reasoning);
                show(message["content"].as_str(), Delta::Reply);
            }
        }
        deltas
    }

    fn add_usage(&mut self, usage: &Value) {
        for (key, value) in usage.as_object().into_iter().flatten() {
            self.usage.insert(key.clone(), value.clone());
        }
    }
}

/// The content block an event is about.
fn block_index(data: &Value) -> Result<usize, String> {
    data["index"]
        .as_u64()
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| "the reply streamed an event without a block index".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};

    /// Prints `value` with sorted keys, whether or not `serde_json` preserves their order.
    fn pretty(value: &Value) -> String {
        fn sorted(value: &Value) -> Value {
            match value {
                Value::Object(object) => {
                    let mut entries = object.iter().collect::<Vec<_>>();
                    entries.sort_by_key(|&(key, _)| key);
                    Value::Object(
                        entries.into_iter().map(|(k, v)| (k.clone(), sorted(v))).collect(),
                    )
                }
                Value::Array(array) => Value::Array(array.iter().map(sorted).collect()),
                _ => value.clone(),
            }
        }
        serde_json::to_string_pretty(&sorted(value)).unwrap()
    }

    #[test]
    fn messages() {
        let mut transcript =
            Transcript::new(Protocol::Messages, "claude-opus-5-5".into(), Some(LlmEffort::High));
        transcript.push_prompt("first");
        let reply = json!({
            "content": [
                {"type": "thinking", "thinking": "", "signature": "sig"},
                {"type": "text", "text": "a"},
                {"type": "text", "text": "b"},
            ],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 5,
                "output_tokens": 7,
                "cache_creation_input_tokens": 11,
                "cache_read_input_tokens": 13,
            },
        });
        let (text, usage) = transcript.push_reply(&reply).unwrap();
        assert_eq!(text, "ab");
        assert_eq!(usage, Usage { input: 5, output: 7, cache_read: 13, cache_write: 11 });
        transcript.push_prompt("second");
        assert_data_eq!(
            pretty(&transcript.request("brief")),
            str![[r#"
{
  "max_tokens": 32000,
  "messages": [
    {
      "content": [
        {
          "text": "first",
          "type": "text"
        }
      ],
      "role": "user"
    },
    {
      "content": [
        {
          "signature": "sig",
          "thinking": "",
          "type": "thinking"
        },
        {
          "text": "a",
          "type": "text"
        },
        {
          "text": "b",
          "type": "text"
        }
      ],
      "role": "assistant"
    },
    {
      "content": [
        {
          "cache_control": {
            "type": "ephemeral"
          },
          "text": "second",
          "type": "text"
        }
      ],
      "role": "user"
    }
  ],
  "model": "claude-opus-5-5",
  "output_config": {
    "effort": "high"
  },
  "stream": true,
  "system": [
    {
      "cache_control": {
        "type": "ephemeral"
      },
      "text": "brief",
      "type": "text"
    }
  ],
  "thinking": {
    "type": "adaptive"
  }
}
"#]]
        );

        let refused = json!({"content": [], "stop_reason": "refusal"});
        assert!(transcript.push_reply(&refused).is_err());
        assert!(transcript.push_reply(&json!({"error": {}})).is_err());
    }

    #[test]
    fn chat_completions() {
        let mut transcript = Transcript::new(
            Protocol::ChatCompletions,
            "deepseek-v4.1-flash".into(),
            Some(LlmEffort::Max),
        );
        transcript.push_prompt("first");
        let reply = json!({
            "choices": [{
                "message": {"role": "assistant", "content": "a", "reasoning_content": "because"},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 20, "completion_tokens": 7, "prompt_cache_hit_tokens": 16},
        });
        let (text, usage) = transcript.push_reply(&reply).unwrap();
        assert_eq!(text, "a");
        assert_eq!(usage, Usage { input: 4, output: 7, cache_read: 16, cache_write: 0 });
        transcript.push_prompt("second");
        let reply = json!({
            "choices": [{"message": {"role": "assistant", "content": null}}],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 1,
                "prompt_tokens_details": {"cached_tokens": 30},
            },
        });
        let (text, usage) = transcript.push_reply(&reply).unwrap();
        assert_eq!(text, "");
        assert_eq!(usage, Usage { input: 0, output: 1, cache_read: 20, cache_write: 0 });
        transcript.push_prompt("third");
        assert_data_eq!(
            pretty(&transcript.request("brief")),
            str![[r#"
{
  "max_tokens": 32000,
  "messages": [
    {
      "content": "brief",
      "role": "system"
    },
    {
      "content": "first",
      "role": "user"
    },
    {
      "content": "a",
      "reasoning_content": "because",
      "role": "assistant"
    },
    {
      "content": "second",
      "role": "user"
    },
    {
      "content": "",
      "role": "assistant"
    },
    {
      "content": "third",
      "role": "user"
    }
  ],
  "model": "deepseek-v4.1-flash",
  "reasoning_effort": "max",
  "stream": true,
  "stream_options": {
    "include_usage": true
  }
}
"#]]
        );
        assert!(transcript.push_reply(&json!({"choices": []})).is_err());
    }

    /// Streams `sse` into a reply of `protocol` in chunks of `size` bytes.
    fn stream(protocol: Protocol, sse: &str, size: usize) -> (Vec<Delta>, Result<Value, String>) {
        let mut reply = Transcript::new(protocol, "m".into(), None).reply_stream();
        let mut parser = SseParser::default();
        let mut deltas = Vec::new();
        for chunk in sse.as_bytes().chunks(size) {
            for event in parser.push(chunk) {
                match reply.read(&event) {
                    Ok(more) => deltas.extend(more),
                    Err(error) => return (deltas, Err(error)),
                }
            }
        }
        if let Some(event) = parser.finish() {
            deltas.extend(reply.read(&event).unwrap());
        }
        (deltas, reply.finish())
    }

    #[test]
    fn server_sent_events() {
        let mut parser = SseParser::default();
        let events = "event: a\r\ndata: 1\r\ndata: 2\r\n\r\n: comment\n\ndata:é\n\ndata: tail";
        let mut seen = Vec::new();
        // One byte at a time splits every line, and the two bytes of `é`.
        for byte in events.as_bytes() {
            seen.extend(parser.push(std::slice::from_ref(byte)));
        }
        seen.extend(parser.finish());
        let event = |name: Option<&str>, data: &str| SseEvent {
            name: name.map(str::to_string),
            data: data.to_string(),
        };
        assert_eq!(seen, [event(Some("a"), "1\n2"), event(None, "é"), event(None, "tail")]);
    }

    #[test]
    fn messages_stream() {
        let sse = r#"event: message_start
data: {"type":"message_start","message":{"usage":{"input_tokens":5,"cache_read_input_tokens":13,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hm"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}

event: ping
data: {"type":"ping"}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"a"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"b"}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}

event: message_stop
data: {"type":"message_stop"}

"#;
        let (deltas, reply) = stream(Protocol::Messages, sse, 7);
        assert_eq!(
            deltas,
            [Delta::Reasoning("hm".into()), Delta::Reply("a".into()), Delta::Reply("b".into())]
        );
        assert_data_eq!(
            pretty(&reply.unwrap()),
            str![[r#"
{
  "content": [
    {
      "signature": "sig",
      "thinking": "hm",
      "type": "thinking"
    },
    {
      "text": "ab",
      "type": "text"
    }
  ],
  "stop_reason": "end_turn",
  "usage": {
    "cache_read_input_tokens": 13,
    "input_tokens": 5,
    "output_tokens": 7
  }
}
"#]]
        );
        // A stream that stops before `message_stop` broke off.
        let cut = &sse[..sse.find("event: message_stop").unwrap()];
        assert!(stream(Protocol::Messages, cut, 64).1.is_err());
        let error =
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"message\":\"overloaded\"}}\n\n";
        assert_eq!(stream(Protocol::Messages, error, 64).1, Err("overloaded".into()));
    }

    #[test]
    fn chat_completions_stream() {
        let sse = r#"data: {"choices":[{"delta":{"role":"assistant","reasoning_content":"hm"}}],"usage":null}

data: {"choices":[{"delta":{"content":"a"}}]}

data: {"choices":[{"delta":{"content":"b"},"finish_reason":"stop"}]}

data: {"choices":[],"usage":{"prompt_tokens":20,"completion_tokens":7,"prompt_cache_hit_tokens":16}}

data: [DONE]

"#;
        let (deltas, reply) = stream(Protocol::ChatCompletions, sse, 5);
        assert_eq!(
            deltas,
            [Delta::Reasoning("hm".into()), Delta::Reply("a".into()), Delta::Reply("b".into())]
        );
        let reply = reply.unwrap();
        assert_data_eq!(
            pretty(&reply),
            str![[r#"
{
  "choices": [
    {
      "finish_reason": "stop",
      "message": {
        "content": "ab",
        "reasoning_content": "hm",
        "role": "assistant"
      }
    }
  ],
  "usage": {
    "completion_tokens": 7,
    "prompt_cache_hit_tokens": 16,
    "prompt_tokens": 20
  }
}
"#]]
        );
        let mut transcript = Transcript::new(Protocol::ChatCompletions, "m".into(), None);
        let (text, usage) = transcript.push_reply(&reply).unwrap();
        assert_eq!(
            (text.as_str(), usage),
            ("ab", Usage { input: 4, output: 7, cache_read: 16, cache_write: 0 })
        );
        // A stream that stops before any finish reason broke off.
        let cut = &sse[..sse.find(r#"data: {"choices":[{"delta":{"content":"b"}"#).unwrap()];
        assert!(stream(Protocol::ChatCompletions, cut, 64).1.is_err());
        let error = "data: {\"error\":{\"message\":\"bad request\"}}\n\n";
        assert_eq!(stream(Protocol::ChatCompletions, error, 64).1, Err("bad request".into()));
    }

    #[test]
    fn malformed_blocks_end_the_reply() {
        let start = |index: &str, block: &str| {
            format!(
                "event: content_block_start\ndata: {{\"type\":\"content_block_start\",\
                 \"index\":{index},\"content_block\":{block}}}\n\n"
            )
        };
        let text = r#"{"type":"text","text":""}"#;
        let cases = [
            (start("4294967296", text), "the reply started block 4294967296 where block 0 was due"),
            (start("1", text), "the reply started block 1 where block 0 was due"),
            (start("0", r#""text""#), "the reply started a block that is not an object"),
            (
                start("0", text) + &start("0", text),
                "the reply started block 0 where block 1 was due",
            ),
        ];
        for (sse, error) in cases {
            assert_eq!(stream(Protocol::Messages, &sse, 64).1, Err(error.into()), "{sse}");
        }
    }
}

//! Conversations in the wire formats of chat providers.
//!
//! Anthropic's Messages API and OpenAI-compatible chat completions take the whole conversation
//! with every request. A [`Transcript`] keeps it in the provider's format: the prompts, and each
//! reply as the provider sent it, so reasoning the provider returns goes back with the reply it
//! led to. Messages requests mark the brief and the newest prompt as prompt-cache breakpoints, so
//! a turn reads the conversation so far from Anthropic's cache instead of paying for it in full.

use super::provider::Usage;
use serde_json::{Value, json};
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
  "reasoning_effort": "max"
}
"#]]
        );
        assert!(transcript.push_reply(&json!({"choices": []})).is_err());
    }
}

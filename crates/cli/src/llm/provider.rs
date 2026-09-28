//! The providers `-Zllm-optimize=live` can ask, and what the compiler knows about their models.
//!
//! `-Zllm-model` names a model as `PROVIDER/MODEL`. A model without a known provider prefix is an
//! OpenAI one, so `gpt-6-sol` and `openai/gpt-6-sol` ask the same model. Prices and reasoning
//! efforts are recorded for the models the documentation names, as their providers publish them;
//! other models are asked all the same, with their tokens reported but not priced.

use solar_config::LlmEffort;

/// A model provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Provider {
    /// OpenAI's Responses API, through nanocodex.
    OpenAi,
    /// Anthropic's Messages API.
    Anthropic,
    /// OpenCode Zen's OpenAI-compatible chat completions API.
    OpenCode,
}

impl Provider {
    /// Splits a `-Zllm-model` value into its provider and model.
    pub(super) fn parse(model: &str) -> (Self, &str) {
        match model.split_once('/') {
            Some(("openai", model)) => (Self::OpenAi, model),
            Some(("anthropic", model)) => (Self::Anthropic, model),
            Some(("opencode", model)) => (Self::OpenCode, model),
            _ => (Self::OpenAi, model),
        }
    }

    /// The provider's name in diagnostics.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::OpenCode => "OpenCode Zen",
        }
    }

    /// The environment variable holding the provider's key.
    pub(super) const fn key_variable(self) -> &'static str {
        match self {
            Self::OpenAi => "OPENAI_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenCode => "OPENCODE_ZEN_API_KEY",
        }
    }

    /// The API base URL that `-Zllm-endpoint` replaces; nanocodex knows OpenAI's.
    pub(super) const fn default_endpoint(self) -> Option<&'static str> {
        match self {
            Self::OpenAi => None,
            Self::Anthropic => Some("https://api.anthropic.com/v1"),
            Self::OpenCode => Some("https://opencode.ai/zen/v1"),
        }
    }
}

/// What the compiler knows about a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ModelInfo {
    /// What its tokens cost.
    pub(super) prices: Prices,
    /// The reasoning efforts it accepts.
    pub(super) efforts: &'static [LlmEffort],
}

/// Token prices in nano-USD per token, so a dollar per million tokens is a thousand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Prices {
    /// An input token billed in full.
    pub(super) input: u64,
    /// An output token, reasoning included.
    pub(super) output: u64,
    /// An input token read from the provider's prompt cache.
    pub(super) cache_read: u64,
    /// An input token written to the provider's prompt cache.
    pub(super) cache_write: u64,
}

impl Prices {
    /// The cost of `usage`, in nano-USD.
    pub(super) fn cost(self, usage: Usage) -> u64 {
        [
            (usage.input, self.input),
            (usage.output, self.output),
            (usage.cache_read, self.cache_read),
            (usage.cache_write, self.cache_write),
        ]
        .into_iter()
        .fold(0u64, |cost, (tokens, price)| cost.saturating_add(tokens.saturating_mul(price)))
    }
}

/// The tokens one turn used, split by how they are billed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Usage {
    /// Input tokens billed in full.
    pub(super) input: u64,
    /// Output tokens, reasoning included.
    pub(super) output: u64,
    /// Input tokens read from the provider's prompt cache.
    pub(super) cache_read: u64,
    /// Input tokens written to the provider's prompt cache.
    pub(super) cache_write: u64,
}

impl Usage {
    /// Every token the turn used.
    pub(super) fn total(self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }
}

/// Published prices and reasoning efforts of the models the documentation names.
const MODELS: &[(Provider, &str, ModelInfo)] = &[
    (
        Provider::Anthropic,
        "claude-opus-5-5",
        ModelInfo {
            prices: Prices { input: 4_000, output: 20_000, cache_read: 200, cache_write: 5_000 },
            efforts: &[
                LlmEffort::Low,
                LlmEffort::Medium,
                LlmEffort::High,
                LlmEffort::Xhigh,
                LlmEffort::Max,
            ],
        },
    ),
    (
        Provider::OpenCode,
        "deepseek-v4.1-flash",
        ModelInfo {
            prices: Prices { input: 300, output: 1_200, cache_read: 6, cache_write: 0 },
            efforts: &[LlmEffort::Low, LlmEffort::High, LlmEffort::Max],
        },
    ),
];

/// Returns what the compiler knows about `model` at `provider`.
pub(super) fn model_info(provider: Provider, model: &str) -> Option<ModelInfo> {
    MODELS.iter().find(|&&(p, m, _)| p == provider && m == model).map(|&(_, _, info)| info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models() {
        assert_eq!(
            Provider::parse("anthropic/claude-opus-5-5"),
            (Provider::Anthropic, "claude-opus-5-5")
        );
        assert_eq!(
            Provider::parse("opencode/deepseek-v4.1-flash"),
            (Provider::OpenCode, "deepseek-v4.1-flash")
        );
        assert_eq!(Provider::parse("openai/gpt-6-sol"), (Provider::OpenAi, "gpt-6-sol"));
        assert_eq!(Provider::parse("gpt-6-sol"), (Provider::OpenAi, "gpt-6-sol"));
        // An unknown prefix is part of an OpenAI model's name.
        assert_eq!(Provider::parse("deepseek/v4"), (Provider::OpenAi, "deepseek/v4"));

        let opus = model_info(Provider::Anthropic, "claude-opus-5-5").unwrap();
        assert!(!opus.efforts.contains(&LlmEffort::None));
        assert!(model_info(Provider::OpenCode, "claude-opus-5-5").is_none());
        let flash = model_info(Provider::OpenCode, "deepseek-v4.1-flash").unwrap();
        assert!(!flash.efforts.contains(&LlmEffort::Medium));
    }

    #[test]
    fn costs() {
        let opus = model_info(Provider::Anthropic, "claude-opus-5-5").unwrap().prices;
        let usage = Usage { input: 1_000, output: 2_000, cache_read: 10_000, cache_write: 3_000 };
        // $4, $20, $0.20, and $5 per million tokens: 0.004 + 0.04 + 0.002 + 0.015 dollars.
        assert_eq!(opus.cost(usage), 61_000_000);
        assert_eq!(usage.total(), 16_000);
        let flash = model_info(Provider::OpenCode, "deepseek-v4.1-flash").unwrap().prices;
        // $0.30, $1.20, and $0.006 per million tokens.
        let usage = Usage {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            ..Usage::default()
        };
        assert_eq!(flash.cost(usage), 1_506_000_000);
    }
}

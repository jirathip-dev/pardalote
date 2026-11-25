//! The conversation pard stores in its log and re-sends on every request, and what a model call gives back.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// One entry of a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Item {
    /// A prompt from the user.
    User(String),
    /// A model's answer, its blocks in the order the model sent them.
    Assistant(Vec<Block>),
    /// The results of every tool call of the answer before it.
    Results(Vec<ToolResult>),
}

/// One part of a model's answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Block {
    /// The model's reasoning, with the provider's signature where it gives one.
    Thinking {
        /// The reasoning text.
        text: String,
        /// The provider's signature over it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// Text for the user.
    Text(String),
    /// A tool call.
    Call(Call),
}

/// A tool call as the model sent it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    /// The call's id, which its result names.
    pub id: String,
    /// The tool the model named, which may not exist.
    pub tool: String,
    /// The arguments exactly as the model wrote them.
    pub input: Box<RawValue>,
}

/// The result of one tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    /// The id of the call it answers.
    pub id: String,
    /// What the model reads.
    pub text: String,
    /// Whether the call failed.
    pub failed: bool,
}

/// Token counts of one model call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Uncached input tokens.
    pub input: u64,
    /// Input tokens read from the provider's cache.
    pub cache_read: u64,
    /// Input tokens written to the provider's cache.
    pub cache_write: u64,
    /// Output tokens.
    pub output: u64,
}

/// An amount of money.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Money {
    /// The currency code, e.g. `USD`.
    pub currency: String,
    /// The amount in that currency.
    pub amount: f64,
}

/// Text that streams while a model call runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    /// Reasoning text.
    Thinking(String),
    /// Answer text.
    Text(String),
}

/// Why the model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// It finished its answer.
    End,
    /// It asked for tool calls.
    Calls,
    /// It hit the output limit.
    MaxTokens,
    /// It refused.
    Refusal,
}

/// A whole answer of one model call.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The answer's blocks, in order.
    pub blocks: Vec<Block>,
    /// Why the model stopped.
    pub stop: Stop,
    /// The call's token counts.
    pub usage: Usage,
    /// The call's cost, where it is known.
    pub cost: Option<Money>,
}

/// Why a model call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// The provider refused the key.
    Auth {
        /// The provider's file name.
        provider: String,
        /// The HTTP status.
        status: u16,
    },
    /// A limit that outlasted the retries.
    Limit {
        /// The provider's file name.
        provider: String,
        /// The HTTP status.
        status: u16,
        /// When the limit resets, as the provider said.
        resets: Option<String>,
    },
    /// The context is full.
    Overflow,
    /// Any other failure, with its message.
    Fatal(String),
    /// The call was cancelled.
    Cancelled,
}

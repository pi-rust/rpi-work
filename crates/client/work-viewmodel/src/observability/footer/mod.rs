//! Footer host contract and the built-in, client-only footer providers.

pub mod registry;

use crate::observability::metrics::{Context, Metrics};
use work_core::extensions::RuntimeFooterSnapshot;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FooterContext {
    pub scope: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub metrics: Metrics,
    pub context: Context,
    pub tool_calls: usize,
    pub tool_failures: usize,
    pub online: bool,
}

impl FooterContext {
    /// Convert the built-in client context to the read-only extension host contract.
    #[must_use]
    pub fn host_snapshot(&self) -> RuntimeFooterSnapshot {
        RuntimeFooterSnapshot {
            online: self.online,
            provider: self.provider.clone(),
            model: self.model.clone(),
            thinking: self.thinking.clone(),
            input_tokens: self.metrics.input,
            output_tokens: self.metrics.output,
            cache_hit_rate: self.metrics.cache_percent,
            cost: self.metrics.cost,
            speed: self.metrics.speed,
            requests: self.metrics.requests,
            retries: self.metrics.retries,
            context_used_percent: self.context.percent,
            tool_calls: u64::try_from(self.tool_calls).ok(),
            tool_failures: u64::try_from(self.tool_failures).ok(),
        }
    }
}

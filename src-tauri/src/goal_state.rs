use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalState {
    pub thread_id: String,
    pub objective: String,
    pub status: String,
    pub token_budget: Option<u64>,
    pub tokens_used: u64,
    pub time_used_seconds: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl GoalState {
    pub fn is_valid(&self) -> bool {
        !self.thread_id.is_empty()
            && self.thread_id.len() <= 200
            && !self.objective.trim().is_empty()
            && self.objective.len() <= 32 * 1024
            && matches!(
                self.status.as_str(),
                "active" | "paused" | "blocked" | "usageLimited" | "budgetLimited" | "complete"
            )
            && self
                .token_budget
                .is_none_or(|n| n > 0 && n <= 9_007_199_254_740_991)
            && self.tokens_used <= 9_007_199_254_740_991
            && self.time_used_seconds <= 9_007_199_254_740_991
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum GoalRequest {
    Set {
        objective: String,
        #[serde(rename = "tokenBudget")]
        token_budget: Option<u64>,
    },
    Get,
    Pause,
    Resume,
    Clear,
}

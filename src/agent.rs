use std::collections::HashMap;

use crate::utils::AppResult;

#[derive(Debug, Clone, Copy)]
pub enum AgentAction {
    Buy { stonk_id: usize, amount: u64 },
    Sell { stonk_id: usize, amount: u64 },
}

pub trait DecisionAgent {
    fn cash(&self) -> u64;
    fn add_cash(&mut self, amount: u64) -> AppResult<u64>;
    fn sub_cash(&mut self, amount: u64) -> AppResult<u64>;
    fn owned_stonks(&self) -> &HashMap<usize, u64>;
    fn add_stonk(&mut self, stonk_id: usize, amount: u64) -> AppResult<&HashMap<usize, u64>>;
    fn sub_stonk(&mut self, stonk_id: usize, amount: u64) -> AppResult<&HashMap<usize, u64>>;

    fn actions(&self) -> &Vec<AgentAction>;
    fn select_action(&mut self, action: AgentAction);
    fn selected_action(&mut self) -> Option<AgentAction>;
}

#[derive(Debug, Clone, Default)]
pub struct UserAgent {
    cash: u64, //in usd cents
    owned_stonks: HashMap<usize, u64>,
    last_actions: Vec<AgentAction>,
    pending_action: Option<AgentAction>,
}

impl UserAgent {
    pub fn new() -> Self {
        Self {
            cash: 10000,
            ..Default::default()
        }
    }

    pub fn formatted_cash(&self) -> f64 {
        self.cash as f64 / 100.0
    }
}

impl DecisionAgent for UserAgent {
    fn cash(&self) -> u64 {
        self.cash
    }
    fn add_cash(&mut self, amount: u64) -> AppResult<u64> {
        self.cash += amount;
        Ok(self.cash)
    }

    fn sub_cash(&mut self, amount: u64) -> AppResult<u64> {
        if self.cash < amount {
            return Err("Underflow".into());
        }
        self.cash -= amount;
        Ok(self.cash)
    }

    fn owned_stonks(&self) -> &HashMap<usize, u64> {
        &self.owned_stonks
    }

    fn add_stonk(&mut self, stonk_id: usize, amount: u64) -> AppResult<&HashMap<usize, u64>> {
        let owned = self
            .owned_stonks
            .get(&stonk_id)
            .copied()
            .unwrap_or_default();
        if let Some(new_amount) = owned.checked_add(amount) {
            self.owned_stonks.insert(stonk_id, new_amount);
        } else {
            return Err("Overflow".into());
        }

        Ok(&self.owned_stonks)
    }

    fn sub_stonk(&mut self, stonk_id: usize, amount: u64) -> AppResult<&HashMap<usize, u64>> {
        let owned = self
            .owned_stonks
            .get(&stonk_id)
            .copied()
            .unwrap_or_default();

        if let Some(new_amount) = owned.checked_sub(amount) {
            self.owned_stonks.insert(stonk_id, new_amount);
        } else {
            return Err("Underflow".into());
        }
        Ok(&self.owned_stonks)
    }

    fn actions(&self) -> &Vec<AgentAction> {
        &self.last_actions
    }

    fn select_action(&mut self, action: AgentAction) {
        self.pending_action = Some(action);
    }

    fn selected_action(&mut self) -> Option<AgentAction> {
        let action = self.pending_action.clone();
        self.pending_action = None;
        if let Some(act) = action {
            self.last_actions.push(act);
        }
        action
    }
}

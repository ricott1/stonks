use crate::utils::AppResult;

const INITIAL_USER_CASH: u32 = 10000;

#[derive(Debug, Clone, Copy)]
pub enum AgentAction {
    Buy { stonk_id: usize, amount: u32 },
    Sell { stonk_id: usize, amount: u32 },
}

pub trait DecisionAgent {
    fn cash(&self) -> u32;
    fn add_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn sub_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn owned_stonks(&self) -> &Vec<u32>;
    fn add_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&Vec<u32>>;
    fn sub_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&Vec<u32>>;

    fn actions(&self) -> &Vec<AgentAction>;
    fn select_action(&mut self, action: AgentAction);
    fn selected_action(&mut self) -> Option<AgentAction>;
    fn clear_action(&mut self);
}

#[derive(Debug, Clone, Default)]
pub struct UserAgent {
    cash: u32, //in usd cents
    owned_stonks: Vec<u32>,
    last_actions: Vec<AgentAction>,
    pending_action: Option<AgentAction>,
}

impl UserAgent {
    pub fn new() -> Self {
        Self {
            cash: INITIAL_USER_CASH * 100, // in cents
            owned_stonks: vec![0].repeat(8),
            ..Default::default()
        }
    }

    pub fn formatted_cash(&self) -> f64 {
        self.cash as f64 / 100.0
    }
}

impl DecisionAgent for UserAgent {
    fn cash(&self) -> u32 {
        self.cash
    }
    fn add_cash(&mut self, amount: u32) -> AppResult<u32> {
        self.cash += amount;
        Ok(self.cash)
    }

    fn sub_cash(&mut self, amount: u32) -> AppResult<u32> {
        if self.cash < amount {
            return Err("Underflow".into());
        }
        self.cash -= amount;
        Ok(self.cash)
    }

    fn owned_stonks(&self) -> &Vec<u32> {
        &self.owned_stonks
    }

    fn add_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&Vec<u32>> {
        let owned = self.owned_stonks[stonk_id];
        if let Some(new_amount) = owned.checked_add(amount) {
            self.owned_stonks[stonk_id] = new_amount;
        } else {
            return Err("Overflow".into());
        }

        Ok(&self.owned_stonks)
    }

    fn sub_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&Vec<u32>> {
        let owned = self.owned_stonks[stonk_id];
        if let Some(new_amount) = owned.checked_sub(amount) {
            self.owned_stonks[stonk_id] = new_amount;
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
        self.pending_action
    }

    fn clear_action(&mut self) {
        if let Some(act) = self.pending_action {
            self.last_actions.push(act);
        }
        self.pending_action = None;
    }
}

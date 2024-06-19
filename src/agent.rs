use crate::{market::NUMBER_OF_STONKS, utils::AppResult};

const INITIAL_USER_CASH: u32 = 10000;

#[derive(Debug, Clone, Copy)]
pub enum DayAction {
    Buy { stonk_id: usize, amount: u32 },
    Sell { stonk_id: usize, amount: u32 },
}

pub trait DecisionAgent {
    fn cash(&self) -> u32;
    fn add_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn sub_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn owned_stonks(&self) -> &[u32; NUMBER_OF_STONKS];
    fn add_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]>;
    fn sub_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]>;

    fn select_day_action(&mut self, action: DayAction);
    fn selected_day_action(&mut self) -> Option<DayAction>;
    fn clear_day_action(&mut self);
}

#[derive(Debug, Clone, Default)]
pub struct UserAgent {
    cash: u32, //in usd cents
    owned_stonks: [u32; NUMBER_OF_STONKS],
    last_actions: Vec<DayAction>,
    pending_action: Option<DayAction>,
}

impl UserAgent {
    pub fn new() -> Self {
        Self {
            cash: INITIAL_USER_CASH * 100, // in cents
            owned_stonks: [0; NUMBER_OF_STONKS],
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

    fn owned_stonks(&self) -> &[u32; NUMBER_OF_STONKS] {
        &self.owned_stonks
    }

    fn add_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]> {
        let owned = self.owned_stonks[stonk_id];
        if let Some(new_amount) = owned.checked_add(amount) {
            self.owned_stonks[stonk_id] = new_amount;
        } else {
            return Err("Overflow".into());
        }

        Ok(&self.owned_stonks)
    }

    fn sub_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]> {
        let owned = self.owned_stonks[stonk_id];
        if let Some(new_amount) = owned.checked_sub(amount) {
            self.owned_stonks[stonk_id] = new_amount;
        } else {
            return Err("Underflow".into());
        }
        Ok(&self.owned_stonks)
    }

    fn select_day_action(&mut self, action: DayAction) {
        self.pending_action = Some(action);
    }

    fn selected_day_action(&mut self) -> Option<DayAction> {
        self.pending_action
    }

    fn clear_day_action(&mut self) {
        if let Some(act) = self.pending_action {
            self.last_actions.push(act);
        }
        self.pending_action = None;
    }
}

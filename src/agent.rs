use std::collections::HashMap;

use crate::{
    events::NightEvent, market::NUMBER_OF_STONKS, ssh_client::SessionAuth, stonk::StonkClass,
    utils::AppResult,
};
use serde::{Deserialize, Serialize};
use strum::Display;
use tracing::info;

const INITIAL_USER_CASH_CENTS: u32 = 10000 * 100;

#[derive(Debug, Display, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentAction {
    Buy { stonk_id: usize, amount: u32 },
    Sell { stonk_id: usize, amount: u32 },
    BumpStonkClass { class: StonkClass },
    CrashAll,
    OneDayUltraVision,
    CrashAgentStonks { username: String },
    AddCash { amount: u32 },
    AcceptBribe,
    AssassinationVictim, // This action is actually used to signal that the user got CharacterAssassinated
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AgentCondition {
    Prison,
    UltraVision,
}

pub trait DecisionAgent {
    fn username(&self) -> &str;

    fn cash(&self) -> u32;
    fn add_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn sub_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn owned_stonks(&self) -> &[u32; NUMBER_OF_STONKS];
    fn add_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]>;
    fn sub_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]>;

    fn select_action(&mut self, action: AgentAction);
    fn selected_action(&self) -> Option<&AgentAction>;
    fn clear_action(&mut self);

    fn set_available_night_events(&mut self, actions: Vec<NightEvent>);
    fn available_night_events(&self) -> &Vec<NightEvent>;

    fn insert_past_selected_actions(&mut self, action: AgentAction, tick: usize);
    fn past_selected_actions(&self) -> &HashMap<String, (usize, usize)>;

    fn apply_conditions(&mut self, current_tick: usize);
    fn add_condition(&mut self, condition: AgentCondition, until_tick: usize);
    fn has_condition(&self, condition: AgentCondition) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAgent {
    pub session_auth: SessionAuth,
    cash: u32, //in usd cents
    owned_stonks: [u32; NUMBER_OF_STONKS],
    pending_action: Option<AgentAction>,
    available_night_events: Vec<NightEvent>,
    // A map of actions selected in the past to (number of times it was selected, last tick it was selected).
    // We use the action string as key to be able to serialize, but lose the enum nested properties.
    past_selected_actions: HashMap<String, (usize, usize)>,
    conditions: Vec<(usize, AgentCondition)>,
}

impl UserAgent {
    pub fn new(session_auth: SessionAuth) -> Self {
        Self {
            session_auth,
            cash: INITIAL_USER_CASH_CENTS, // in cents
            owned_stonks: [0; NUMBER_OF_STONKS],
            pending_action: None,
            available_night_events: vec![],
            past_selected_actions: HashMap::default(),
            conditions: vec![],
        }
    }

    pub fn cash_dollars(&self) -> f64 {
        self.cash as f64 / 100.0
    }

    pub fn conditions(&self) -> &Vec<(usize, AgentCondition)> {
        &self.conditions
    }
}

impl DecisionAgent for UserAgent {
    fn username(&self) -> &str {
        &self.session_auth.username
    }

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

    fn select_action(&mut self, action: AgentAction) {
        info!("Agent selected action: {:#?}", action);
        if self.pending_action.is_none() {
            self.pending_action = Some(action);
        }
    }

    fn selected_action(&self) -> Option<&AgentAction> {
        self.pending_action.as_ref()
    }

    fn clear_action(&mut self) {
        self.pending_action = None;
    }

    fn set_available_night_events(&mut self, events: Vec<NightEvent>) {
        self.available_night_events = events;
    }

    fn available_night_events(&self) -> &Vec<NightEvent> {
        &self.available_night_events
    }

    fn insert_past_selected_actions(&mut self, action: AgentAction, tick: usize) {
        if let Some((amount, _)) = self.past_selected_actions.get(&action.to_string()) {
            self.past_selected_actions
                .insert(action.to_string(), (amount + 1, tick));
        } else {
            self.past_selected_actions
                .insert(action.to_string(), (1, tick));
        }
    }

    fn past_selected_actions(&self) -> &HashMap<String, (usize, usize)> {
        &self.past_selected_actions
    }

    fn apply_conditions(&mut self, current_tick: usize) {
        for (_, condition) in self.conditions.iter() {
            match condition {
                AgentCondition::Prison => {}
                AgentCondition::UltraVision => {}
            }
        }

        self.conditions
            .retain(|(until_tick, _)| *until_tick > current_tick);
    }

    fn add_condition(&mut self, condition: AgentCondition, until_tick: usize) {
        self.conditions.push((until_tick, condition));
    }

    fn has_condition(&self, condition: AgentCondition) -> bool {
        self.conditions
            .iter()
            .map(|(_, condition)| *condition)
            .collect::<Vec<AgentCondition>>()
            .contains(&condition)
    }
}

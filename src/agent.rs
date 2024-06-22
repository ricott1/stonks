use crate::{
    market::NUMBER_OF_STONKS, ssh_server::SessionAuth, stonk::StonkClass, utils::AppResult,
};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use strum_macros::EnumIter;

const INITIAL_USER_CASH_CENTS: u32 = 10000 * 100;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AgentAction {
    Buy { stonk_id: usize, amount: u32 },
    Sell { stonk_id: usize, amount: u32 },
    BumpStonkClass { class: StonkClass },
    CrashAll,
}

#[derive(Debug, Clone, Copy, EnumIter, PartialEq, Serialize, Deserialize)]
pub enum NightEvent {
    War,
    ColdWinter,
    MarketCrash,
}

impl Display for NightEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::War => write!(f, "War"),
            Self::ColdWinter => write!(f, "Cold winter"),
            Self::MarketCrash => write!(f, "Market crash"),
        }
    }
}

impl NightEvent {
    pub fn description(&self) -> Vec<&str> {
        match self {
            Self::War => vec![
                "WAR",
                "",
                "It's war time!",
                "Chance for all war stonks",
                "to get a big bump.",
            ],
            Self::ColdWinter => vec![
                "COLD WINTER",
                "",
                "Apparently next winter",
                "is gonna be very cold,",
                "better prepare soon. So",
                "much for global warming!",
            ],
            Self::MarketCrash => vec![
                "MARKET CRASH",
                "",
                "It's 1929 all over again,",
                "or was it 1987?",
                "Or 2001? Or 2008?",
                "Or...",
            ],
        }
    }

    pub fn condition(&self) -> Box<dyn Fn(&dyn DecisionAgent) -> bool> {
        match self {
            Self::War => Box::new(|agent| agent.cash() > 10),
            Self::ColdWinter => Box::new(|agent| agent.cash() > 10),
            Self::MarketCrash => Box::new(|agent| agent.cash() > 10),
        }
    }

    pub fn action(&self) -> AgentAction {
        match self {
            Self::War => AgentAction::BumpStonkClass {
                class: StonkClass::War,
            },
            Self::ColdWinter => AgentAction::BumpStonkClass {
                class: StonkClass::Commodity,
            },
            Self::MarketCrash => AgentAction::CrashAll,
        }
    }
}

pub trait DecisionAgent {
    fn cash(&self) -> u32;
    fn add_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn sub_cash(&mut self, amount: u32) -> AppResult<u32>;
    fn owned_stonks(&self) -> &[u32; NUMBER_OF_STONKS];
    fn add_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]>;
    fn sub_stonk(&mut self, stonk_id: usize, amount: u32) -> AppResult<&[u32; NUMBER_OF_STONKS]>;

    fn select_action(&mut self, action: AgentAction);
    fn selected_action(&self) -> Option<AgentAction>;
    fn clear_action(&mut self);

    fn set_available_night_events(&mut self, actions: Vec<NightEvent>);
    fn available_night_events(&self) -> &Vec<NightEvent>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAgent {
    pub session_auth: SessionAuth,
    cash: u32, //in usd cents
    owned_stonks: [u32; NUMBER_OF_STONKS],
    pending_action: Option<AgentAction>,
    available_night_events: Vec<NightEvent>,
}

impl UserAgent {
    pub fn new(session_auth: SessionAuth) -> Self {
        Self {
            session_auth,
            cash: INITIAL_USER_CASH_CENTS, // in cents
            owned_stonks: [0; NUMBER_OF_STONKS],
            pending_action: None,
            available_night_events: vec![],
        }
    }

    pub fn username(&self) -> &str {
        &self.session_auth.username
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

    fn select_action(&mut self, action: AgentAction) {
        if self.pending_action.is_none() {
            self.pending_action = Some(action);
        }
    }

    fn selected_action(&self) -> Option<AgentAction> {
        self.pending_action
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
}

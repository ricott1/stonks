use crate::{
    market::{Market, NUMBER_OF_STONKS},
    ssh_server::SessionAuth,
    stonk::{Stonk, StonkClass},
    utils::AppResult,
};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use strum_macros::EnumIter;
use tracing::info;

const INITIAL_USER_CASH_CENTS: u32 = 10000 * 100;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AgentAction {
    Buy { stonk_id: usize, amount: u32 },
    Sell { stonk_id: usize, amount: u32 },
    BumpStonkClass { class: StonkClass },
    CrashAll,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AgentCondition {
    Prison { until_tick: usize },
}

#[derive(Debug, Clone, Copy, EnumIter, PartialEq, Serialize, Deserialize)]
pub enum NightEvent {
    War,
    ColdWinter,
    RoyalScandal,
    PurpleBlockchain,
    MarketCrash,
}

impl Display for NightEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::War => write!(f, "War"),
            Self::ColdWinter => write!(f, "Cold winter"),
            Self::RoyalScandal => write!(f, "Royal scandal"),
            Self::PurpleBlockchain => write!(f, "Purple blockchain"),
            Self::MarketCrash => write!(f, "Market crash"),
        }
    }
}

impl NightEvent {
    pub fn title(&self) -> &str {
        match self {
            Self::War => "WAR",
            Self::ColdWinter => "COLD WINTER",
            Self::RoyalScandal => "ROYAL SCANDAL",
            Self::PurpleBlockchain => "PURPLE BLOCKCHAIN",
            Self::MarketCrash => "MARKET CRASH",
        }
    }

    pub fn description(&self) -> Vec<&str> {
        let mut description = match self {
            Self::War => vec![
                "It's war time!",
                "Chance for all war stonks",
                "to get a big bump.",
            ],
            Self::ColdWinter => vec![
                "Apparently next winter",
                "is gonna be very cold,",
                "better prepare soon. So",
                "much for global warming!",
            ],
            Self::RoyalScandal => vec![
                "A juicy scandal will hit",
                "every frontpage tomorrow.",
                "Media stonks will surely",
                "sell some extra!",
            ],
            Self::PurpleBlockchain => vec![
                "Didn't you hear?",
                "Blockchains are gonna smash",
                "away the rotten banks.",
                "Just put it on chain,",
                "and make it purple.",
            ],
            Self::MarketCrash => vec![
                "It's 1929 all over again,",
                "or was it 1987?",
                "Or 2001? Or 2008?",
                "Or...",
            ],
        };

        description.extend(vec!["", "Unlock Condition:"].iter());
        description.extend(self.unlock_condition_description().iter());

        description
    }

    pub fn unlock_condition(&self) -> Box<dyn Fn(&dyn DecisionAgent, &Market) -> bool> {
        match self {
            Self::War => Box::new(|agent, market| {
                let war_stonks = market
                    .stonks
                    .iter()
                    .filter(|s| s.class == StonkClass::War)
                    .collect::<Vec<&Stonk>>();

                war_stonks
                    .iter()
                    .map(|s| 100.0 * s.to_stake(agent.owned_stonks()[s.id]))
                    .sum::<f64>()
                    / war_stonks.len() as f64
                    >= 1.0
            }),
            Self::ColdWinter => Box::new(|agent, market| {
                let commodity_stonks = market
                    .stonks
                    .iter()
                    .filter(|s| s.class == StonkClass::Commodity)
                    .collect::<Vec<&Stonk>>();

                commodity_stonks
                    .iter()
                    .map(|s| 100.0 * s.to_stake(agent.owned_stonks()[s.id]))
                    .sum::<f64>()
                    / commodity_stonks.len() as f64
                    >= 1.0
            }),
            Self::RoyalScandal => Box::new(|agent, market| {
                let media_stonks = market
                    .stonks
                    .iter()
                    .filter(|s| s.class == StonkClass::Media)
                    .collect::<Vec<&Stonk>>();

                media_stonks
                    .iter()
                    .map(|s| 100.0 * s.to_stake(agent.owned_stonks()[s.id]))
                    .sum::<f64>()
                    / media_stonks.len() as f64
                    >= 1.0
            }),
            Self::PurpleBlockchain => Box::new(|agent, market| {
                let tech_stonks = market
                    .stonks
                    .iter()
                    .filter(|s| s.class == StonkClass::Technology)
                    .collect::<Vec<&Stonk>>();

                tech_stonks
                    .iter()
                    .map(|s| 100.0 * s.to_stake(agent.owned_stonks()[s.id]))
                    .sum::<f64>()
                    / tech_stonks.len() as f64
                    >= 1.0
            }),
            Self::MarketCrash => Box::new(|agent, _| agent.cash() >= 100_000 * 100),
        }
    }

    fn unlock_condition_description(&self) -> Vec<&str> {
        match self {
            Self::War => vec!["Average share in", "War stonks >= 1%"],
            Self::ColdWinter => vec!["Average share in", "Commodity stonks >= 1%"],
            Self::RoyalScandal => vec!["Average share in", "Media stonks >= 1%"],
            Self::PurpleBlockchain => vec!["Average share in", "Technology stonks >= 1%"],
            Self::MarketCrash => vec!["Total cash >= $100000"],
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
            Self::RoyalScandal => AgentAction::BumpStonkClass {
                class: StonkClass::Media,
            },
            Self::PurpleBlockchain => AgentAction::BumpStonkClass {
                class: StonkClass::Technology,
            },
            Self::MarketCrash => AgentAction::CrashAll,
        }
    }
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
    conditions: Vec<AgentCondition>,
}

impl UserAgent {
    pub fn new(session_auth: SessionAuth) -> Self {
        Self {
            session_auth,
            cash: INITIAL_USER_CASH_CENTS, // in cents
            owned_stonks: [0; NUMBER_OF_STONKS],
            pending_action: None,
            available_night_events: vec![],
            conditions: vec![],
        }
    }

    pub fn formatted_cash(&self) -> f64 {
        self.cash as f64 / 100.0
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
        if self.pending_action.is_none() {
            self.pending_action = Some(action);
        }
        info!("Agent selected action: {:#?}", action);
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

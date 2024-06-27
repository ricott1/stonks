use crate::{
    agent::{AgentAction, DecisionAgent},
    market::Market,
    stonk::{Stonk, StonkClass},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

const A_GOOD_OFFER_PROBABILITY: f64 = 0.99994;

#[derive(Debug, Clone, EnumIter, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NightEvent {
    War,
    ColdWinter,
    RoyalScandal,
    PurpleBlockchain,
    MarketCrash,
    UltraVision,
    CharacterAssassination { username: String },
    AGoodOffer,
}

impl Display for NightEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::War => write!(f, "War"),
            Self::ColdWinter => write!(f, "Cold winter"),
            Self::RoyalScandal => write!(f, "Royal scandal"),
            Self::PurpleBlockchain => write!(f, "Purple blockchain"),
            Self::MarketCrash => write!(f, "Market crash"),
            Self::UltraVision => write!(f, "UltraVision"),
            Self::CharacterAssassination { .. } => write!(f, "Character assassination"),
            Self::AGoodOffer => write!(f, "A good offer"),
        }
    }
}

impl NightEvent {
    pub fn description(&self) -> Vec<String> {
        let mut description = match self {
            Self::War => vec![
                "It's war time!".to_string(),
                "Chance for all war stonks".to_string(),
                "to get a big bump.".to_string(),
            ],
            Self::ColdWinter => vec![
                "Apparently next winter".to_string(),
                "is gonna be very cold,".to_string(),
                "better prepare soon. So".to_string(),
                "much for global warming!".to_string(),
            ],
            Self::RoyalScandal => vec![
                "A juicy scandal will hit".to_string(),
                "every frontpage tomorrow.".to_string(),
                "Media stonks will surely".to_string(),
                "sell some extra!".to_string(),
            ],
            Self::PurpleBlockchain => vec![
                "Didn't you hear?".to_string(),
                "Blockchains are gonna ruin".to_string(),
                "the broken financial".to_string(),
                "system. Just put it on".to_string(),
                "chain, and make it purple.".to_string(),
            ],
            Self::MarketCrash => vec![
                "It's 1929 all over again,".to_string(),
                "or was it 1987?".to_string(),
                "Or 2001? Or 2008?".to_string(),
                "Or...".to_string(),
            ],
            Self::UltraVision => vec![
                "You woke up differently".to_string(),
                "this morning, with a sense".to_string(),
                "of prescience about".to_string(),
                "something incoming...".to_string(),
            ],
            Self::CharacterAssassination { username } => {
                vec![
                    format!("That fucker {}", username),
                    "better pay attention".to_string(),
                    "to their stonks tomorrow.".to_string(),
                ]
            }
            Self::AGoodOffer => vec![
                "An offer you can't refuse".to_string(),
                "they say. Get $10000,".to_string(),
                "pay later (maybe).".to_string(),
            ],
        };

        description.push("".to_string());
        description.push("Unlock Condition:".to_string());
        for l in self.unlock_condition_description().iter() {
            description.push(l.clone());
        }

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
            Self::UltraVision => Box::new(|agent, market| {
                let riccardino_id = 3;
                let riccardino = &market.stonks[riccardino_id];
                100.0 * riccardino.to_stake(agent.owned_stonks()[riccardino_id]) >= 10.0
            }),
            Self::CharacterAssassination { username, .. } => {
                let username = username.clone();
                Box::new(move |agent, _| {
                    // let has_any_large_stake = agent_stonks
                    //     .iter()
                    //     .enumerate()
                    //     .map(|(stonk_id, &amount)| 100.0 * market.stonks[stonk_id].to_stake(amount))
                    //     .any(|s| s > 5.0);
                    username != agent.username()
                    // && has_any_large_stake
                })
            }
            Self::AGoodOffer => Box::new(|agent, _| {
                agent
                    .past_selected_actions()
                    .get(&AgentAction::AcceptBribe.to_string())
                    .is_none()
                    && agent.cash() < 1000 * 100
                    && {
                        let rng = &mut rand::thread_rng();
                        rng.gen_bool(A_GOOD_OFFER_PROBABILITY)
                    }
            }),
        }
    }

    fn unlock_condition_description(&self) -> Vec<String> {
        match self {
            Self::War => vec![
                "Average share in".to_string(),
                "War stonks >= 1%".to_string(),
            ],
            Self::ColdWinter => vec![
                "Average share in".to_string(),
                "Commodity stonks >= 1%".to_string(),
            ],
            Self::RoyalScandal => vec![
                "Average share in".to_string(),
                "Media stonks >= 1%".to_string(),
            ],
            Self::PurpleBlockchain => vec![
                "Average share in".to_string(),
                "Technology stonks >= 1%".to_string(),
            ],
            Self::MarketCrash => vec!["Total cash >= $100000".to_string()],
            Self::UltraVision => vec!["Riccardino share >= 10%".to_string()],
            Self::CharacterAssassination { username, .. } => vec![
                format!("{username} took a special offer"),
                "in the past and got too".to_string(),
                "greedy now.".to_string(),
            ],
            Self::AGoodOffer => vec![
                "Random chance,".to_string(),
                "happens only once".to_string(),
            ],
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
            Self::UltraVision => AgentAction::OneDayUltraVision,
            Self::CharacterAssassination { username, .. } => AgentAction::CrashAgentStonks {
                username: username.to_string(),
            },
            Self::AGoodOffer => AgentAction::AcceptBribe,
        }
    }
}

use std::collections::HashMap;

use crate::{
    agent::{AgentAction, AgentCondition, DecisionAgent, UserAgent, INITIAL_USER_CASH_CENTS},
    events::CHARACTER_ASSASSINATION_COST,
    stonk::{Stonk, StonkCondition},
    utils::{load_stonks_data, AppResult},
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{debug, info};

const DAY_STARTING_HOUR: usize = 6;
const DAY_LENGTH_HOURS: usize = 16;
const NIGHT_LENGTH_HOURS: usize = 24 - DAY_LENGTH_HOURS;

const TICKS_PER_HOUR: usize = 4;
pub const MAX_EVENTS_PER_NIGHT: usize = 3;

// Each second represents 15 minutes => 1 hour = 4 ticks.
pub const DAY_LENGTH: usize = TICKS_PER_HOUR * DAY_LENGTH_HOURS; // DAY_LENGTH = 16 hours
pub const NIGHT_LENGTH: usize = TICKS_PER_HOUR * NIGHT_LENGTH_HOURS; // NIGHT_LENGTH = 8 hours

// We keep record of the last 12 weeks
pub const HISTORICAL_SIZE: usize = DAY_LENGTH * 7 * 12;
pub const NUMBER_OF_STONKS: usize = 8;

const BRIBE_AMOUNT: u32 = 10_000 * 100;

const GLOBAL_DRIFT_VOLATILITY: f64 = 1.0;

#[derive(Debug, Clone, Copy, Display, EnumIter)]
enum Season {
    Spring,
    Summer,
    Fall,
    Winter,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum GamePhase {
    Day { cycle: usize, counter: usize },
    Night { cycle: usize, counter: usize },
}

impl GamePhase {
    fn time(&self) -> (usize, usize) {
        match self {
            Self::Day { counter, .. } => (
                (DAY_STARTING_HOUR + counter / TICKS_PER_HOUR) % 24,
                (counter % TICKS_PER_HOUR) * 15,
            ),
            Self::Night { counter, .. } => (
                (DAY_STARTING_HOUR + DAY_LENGTH_HOURS + counter / TICKS_PER_HOUR) % 24,
                (counter % TICKS_PER_HOUR) * 15,
            ),
        }
    }

    fn day(&self) -> usize {
        match self {
            Self::Day { cycle, .. } => cycle % 365 + 1,
            Self::Night { cycle, .. } => cycle % 365 + 1,
        }
    }

    fn season(&self) -> Season {
        let seasons = Season::iter().collect::<Vec<Season>>();
        match self {
            Self::Day { cycle, .. } => seasons[(cycle / 90) % 4],
            Self::Night { cycle, .. } => seasons[(cycle / 90) % 4],
        }
    }

    fn year(&self) -> usize {
        2024 + match self {
            Self::Day { cycle, .. } => cycle / 90 / 4 + 1,
            Self::Night { cycle, .. } => cycle / 90 / 4 + 1,
        }
    }

    pub fn formatted(&self) -> String {
        let time = self.time();
        format!(
            "{:3} {:6} {:4} {:02}:{:02}",
            self.day(),
            self.season(),
            self.year(),
            time.0,
            time.1
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub stonks: [Stonk; NUMBER_OF_STONKS],
    pub last_tick: usize,
    pub phase: GamePhase,
    initial_total_market_cap: u64,
    #[serde(default)]
    target_total_market_cap: u64,
    #[serde(default)]
    pub portfolios: Vec<(String, u64)>,
}

impl Default for Market {
    fn default() -> Self {
        Self::new()
    }
}

impl Market {
    pub fn new() -> Self {
        let stonks = load_stonks_data().expect("Failed to load stonks from data");

        let mut m = Market {
            stonks,
            last_tick: 0,
            phase: GamePhase::Day {
                cycle: 0,
                counter: 0,
            },
            initial_total_market_cap: 0,
            target_total_market_cap: 0,
            portfolios: vec![],
        };

        m.initial_total_market_cap = m.total_market_cap();
        m.target_total_market_cap = m.initial_total_market_cap;

        debug!("Started Market with {} stonks!", m.stonks.len());
        for stonk in m.stonks.iter() {
            info!(
                "Stonk availability: {} out of {} ({} bought)",
                stonk.available_amount(),
                stonk.number_of_shares,
                stonk.allocated_shares
            );
        }

        info!(
            "Current total market cap: ${:.2}",
            m.total_market_cap_dollars()
        );

        m
    }

    pub fn total_market_cap(&self) -> u64 {
        self.stonks
            .iter()
            .map(|stonk| stonk.market_cap() as u64)
            .sum::<u64>()
    }

    pub fn total_market_cap_dollars(&self) -> f64 {
        self.total_market_cap() as f64 / 100.0
    }

    pub fn update_target_total_market_cap(&mut self, number_of_agents: usize) -> u64 {
        self.target_total_market_cap = self.initial_total_market_cap
            + number_of_agents as u64 * INITIAL_USER_CASH_CENTS as u64;
        self.target_total_market_cap
    }

    pub fn update_portfolios(
        &mut self,
        agents: &HashMap<String, UserAgent>,
    ) -> &Vec<(String, u64)> {
        let mut portfolios = vec![];
        for (username, agent) in agents.iter() {
            let agent_stonk_value = agent
                .owned_stonks()
                .iter()
                .enumerate()
                .map(|(stonk_id, amount)| {
                    let stonk = &self.stonks[stonk_id];
                    stonk.price_per_share_in_cents as u64 * *amount as u64
                })
                .sum::<u64>();
            if agent_stonk_value > 0 {
                portfolios.push((username.clone(), agent_stonk_value));
            }
        }

        portfolios.sort_by(|(_, a), (_, b)| b.cmp(a));

        self.portfolios = portfolios;

        &self.portfolios
    }

    pub fn tick_day(&mut self, rng: &mut ChaCha8Rng) {
        let current_market_cap = self.total_market_cap() as f64;
        let mean = ((self.target_total_market_cap as f64 - current_market_cap)
            / current_market_cap.min(self.target_total_market_cap as f64))
        .min(5.0)
        .max(-5.0);

        let global_drift = if self.last_tick % DAY_LENGTH == 0 {
            let drift = mean + rng.gen_range(-GLOBAL_DRIFT_VOLATILITY..GLOBAL_DRIFT_VOLATILITY);

            info!(
                "Global drift: current cap {}, target cap {}, global drift {}",
                current_market_cap, self.target_total_market_cap, drift
            );
            Some(drift)
        } else {
            None
        };
        for stonk in self.stonks.iter_mut() {
            if let Some(drift) = global_drift {
                stonk.add_condition(
                    StonkCondition::Bump { amount: drift },
                    self.last_tick + DAY_LENGTH,
                );
            }
            stonk.tick(self.last_tick);
            while stonk.historical_prices.len() > HISTORICAL_SIZE {
                stonk.historical_prices.remove(0);
            }
        }
        self.last_tick += 1;
    }

    fn tick_night(&mut self, _rng: &mut ChaCha8Rng) {}

    pub fn tick(&mut self) {
        debug!("\nMarket tick {:?}", self.phase);
        for stonk in self.stonks.iter() {
            debug!(
                "Stonk availability: {} out of {} ({} bought)",
                stonk.available_amount(),
                stonk.number_of_shares,
                stonk.allocated_shares
            );
        }
        let rng = &mut ChaCha8Rng::from_entropy();
        match self.phase {
            GamePhase::Day { cycle, counter } => {
                self.tick_day(rng);
                if counter < DAY_LENGTH - 1 {
                    self.phase = GamePhase::Day {
                        cycle,
                        counter: counter + 1,
                    }
                } else {
                    self.phase = GamePhase::Night { cycle, counter: 0 }
                }
            }
            GamePhase::Night { cycle, counter } => {
                self.tick_night(rng);
                if counter < NIGHT_LENGTH - 1 {
                    self.phase = GamePhase::Night {
                        cycle,
                        counter: counter + 1,
                    };
                } else {
                    self.phase = GamePhase::Day {
                        cycle: cycle + 1,
                        counter: 0,
                    };
                }
            }
        }
    }

    pub fn apply_agent_action<A: DecisionAgent>(
        &mut self,
        agent: &mut A,
        agents: &mut HashMap<String, A>,
    ) -> AppResult<()> {
        if let Some(action) = agent.selected_action().cloned().as_ref() {
            agent.clear_action();
            info!("Applying action {:?}", action);

            match action {
                AgentAction::Buy { stonk_id, amount } => {
                    let stonk = &mut self.stonks[*stonk_id];
                    let max_amount = stonk.available_amount();
                    if max_amount < *amount {
                        return Err("Not enough shares available".into());
                    }
                    let cost = stonk.buy_price() * amount;
                    agent.sub_cash(cost)?;
                    agent.add_stonk(*stonk_id, *amount)?;

                    stonk.allocate_shares(agent.username(), *amount)?;

                    info!(
                        "{} stonks bought, there are now {} available ({} total bought)",
                        amount,
                        stonk.available_amount(),
                        stonk.allocated_shares
                    );

                    let bump_amount = stonk.to_stake(*amount) * 100.0;
                    stonk.add_condition(
                        StonkCondition::Bump {
                            amount: bump_amount,
                        },
                        self.last_tick + 1,
                    );
                }
                AgentAction::Sell { stonk_id, amount } => {
                    let stonk = &mut self.stonks[*stonk_id];
                    let cost = stonk.sell_price() * amount;
                    agent.sub_stonk(*stonk_id, *amount)?;
                    agent.add_cash(cost)?;
                    stonk.deallocate_shares(agent.username(), *amount)?;

                    info!(
                        "{} stonks sold, there are now {} available ({} total bought)",
                        amount,
                        stonk.available_amount(),
                        stonk.allocated_shares
                    );

                    let bump_amount = stonk.to_stake(*amount) * 100.0;
                    stonk.add_condition(
                        StonkCondition::Bump {
                            amount: -bump_amount,
                        },
                        self.last_tick + 1,
                    );
                }
                AgentAction::BumpStonkClass { class } => {
                    for stonk in self.stonks.iter_mut().filter(|s| s.class == *class) {
                        stonk.add_condition(
                            StonkCondition::Bump { amount: 4.0 },
                            self.last_tick + DAY_LENGTH,
                        )
                    }
                }
                AgentAction::CrashAll => {
                    for stonk in self.stonks.iter_mut() {
                        stonk.add_condition(
                            StonkCondition::Bump { amount: -4.0 },
                            self.last_tick + DAY_LENGTH,
                        );
                        stonk.add_condition(
                            StonkCondition::SetShockProbability {
                                value: 0.25,
                                previous_shock_probability: stonk.shock_probability,
                            },
                            self.last_tick + DAY_LENGTH,
                        )
                    }
                }
                AgentAction::AddCash { amount } => {
                    agent.add_cash(*amount)?;
                }

                AgentAction::AcceptBribe => {
                    agent.add_cash(BRIBE_AMOUNT)?;
                }

                AgentAction::OneDayUltraVision => {
                    agent.add_condition(AgentCondition::UltraVision, self.last_tick + DAY_LENGTH)
                }
                AgentAction::CrashAgentStonks { username } => {
                    if let Some(target) = agents.get_mut(username) {
                        target.insert_past_selected_actions(
                            AgentAction::AssassinationVictim,
                            self.last_tick,
                        );

                        for (stonk_id, &amount) in target.owned_stonks().iter().enumerate() {
                            let stonk = &mut self.stonks[stonk_id];
                            let stake = stonk.to_stake(amount);
                            stonk.add_condition(
                                StonkCondition::Bump {
                                    amount: 10.0 * stake,
                                },
                                self.last_tick + DAY_LENGTH,
                            );
                            stonk.add_condition(
                                StonkCondition::SetShockProbability {
                                    value: (stonk.shock_probability * 4.0).min(1.0),
                                    previous_shock_probability: stonk.shock_probability,
                                },
                                self.last_tick + DAY_LENGTH,
                            );
                        }
                        agent.sub_cash(CHARACTER_ASSASSINATION_COST)?;
                    }
                }
                AgentAction::AssassinationVictim => {}
            }
            agent.insert_past_selected_actions(action.clone(), self.last_tick);
        }
        Ok(())
    }
}

use crate::{
    agent::{AgentAction, AgentCondition, DecisionAgent},
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
            "{:3} {:6} {:3   } {:02}:{:02}",
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
}

impl Default for Market {
    fn default() -> Self {
        Self::new()
    }
}

impl Market {
    pub fn new() -> Self {
        let stonks = load_stonks_data().expect("Failed to load stonks from data");

        let m = Market {
            stonks,
            last_tick: 0,
            phase: GamePhase::Day {
                cycle: 0,
                counter: 0,
            },
        };

        debug!("Started Market with {} stonks!", m.stonks.len());

        m
    }

    pub fn tick_day(&mut self, rng: &mut ChaCha8Rng) {
        let global_drift = if self.last_tick % DAY_LENGTH == 0 {
            Some(rng.gen_range(-0.01..0.01))
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

    pub fn apply_agent_action<A: DecisionAgent>(&mut self, agent: &mut A) -> AppResult<()> {
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

                    let bump_amount = stonk.to_stake(agent.owned_stonks()[stonk.id]);
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

                    let bump_amount = stonk.to_stake(agent.owned_stonks()[stonk.id]);
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
                            StonkCondition::Bump { amount: 5.0 },
                            self.last_tick + DAY_LENGTH,
                        )
                    }
                }
                AgentAction::CrashAll => {
                    for stonk in self.stonks.iter_mut() {
                        stonk.add_condition(
                            StonkCondition::Bump { amount: -5.0 },
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
                AgentAction::CrashAgentStonks { agent_stonks } => {
                    for (stonk_id, &amount) in agent_stonks.iter().enumerate() {
                        let stonk = &mut self.stonks[stonk_id];
                        let stake = stonk.to_stake(amount);
                        stonk.add_condition(
                            StonkCondition::Bump { amount: stake },
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
                }
            }
            agent.insert_past_selected_actions(action.clone(), self.last_tick);
        }
        Ok(())
    }
}

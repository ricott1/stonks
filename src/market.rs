use crate::{
    agent::{AgentAction, DecisionAgent},
    stonk::{Stonk, StonkClass, StonkCondition},
    utils::AppResult,
};
use rand::Rng;
use tracing::debug;

const DAY_STARTING_HOUR: usize = 6;
const DAY_LENGTH_HOURS: usize = 16;
const NIGHT_LENGTH_HOURS: usize = 24 - DAY_LENGTH_HOURS;

// Each second represents 15 minutes => 1 hour = 4 ticks.
pub const DAY_LENGTH: usize = 4 * DAY_LENGTH_HOURS; // DAY_LENGTH = 16 hours
pub const NIGHT_LENGTH: usize = 4 * NIGHT_LENGTH_HOURS; // NIGHT_LENGTH = 8 hours

// We keep record of the last 8 weeks
pub const HISTORICAL_SIZE: usize = DAY_LENGTH * 7 * 8;

pub const NUMBER_OF_STONKS: usize = 8;

#[derive(Debug, Clone, Copy)]
pub enum GamePhase {
    Day { counter: usize },
    Night { counter: usize },
}

impl GamePhase {
    pub fn formatted_time(&self) -> String {
        match self {
            Self::Day { counter } => {
                format!(
                    "{:02}:{:02}",
                    (DAY_STARTING_HOUR + (DAY_LENGTH - counter) / 4) % 24,
                    (DAY_LENGTH - counter) % 4 * 15
                )
            }
            Self::Night { counter } => {
                format!(
                    "{:02}:{:02}",
                    (DAY_STARTING_HOUR + DAY_LENGTH_HOURS + (NIGHT_LENGTH - counter) / 4) % 24,
                    (NIGHT_LENGTH - counter) % 4 * 15
                )
            }
        }
    }
}

pub trait StonkMarket {
    fn tick(&mut self);
    fn apply_agent_action<A: DecisionAgent>(&mut self, agent: &mut A) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub struct Market {
    pub stonks: [Stonk; NUMBER_OF_STONKS],
    pub last_tick: usize,
    pub phase: GamePhase,
    pub cycles: usize,
}

impl Market {
    pub fn new() -> Self {
        let stonks = [
            Market::new_stonk(
                0,
                StonkClass::Technology,
                "Cassius INC".into(),
                9800,
                2500,
                0.0001,
                0.00025,
                0.0008,
                0.12,
            ),
            Market::new_stonk(
                1,
                StonkClass::Technology,
                "Tesla".into(),
                10000,
                250,
                0.01,
                0.00025,
                0.001,
                0.01,
            ),
            Market::new_stonk(
                2,
                StonkClass::Commodity,
                "Rovanti".into(),
                8000,
                250,
                0.005,
                0.0005,
                0.00075,
                0.01,
            ),
            Market::new_stonk(
                3,
                StonkClass::Media,
                "Riccardino".into(),
                9000,
                10000,
                0.000,
                0.00025,
                0.00075,
                0.07,
            ),
            Market::new_stonk(
                4,
                StonkClass::War,
                "Mariottide".into(),
                80000,
                1000,
                0.000,
                0.00025,
                0.001,
                0.1,
            ),
            Market::new_stonk(
                5,
                StonkClass::War,
                "Cubbit".into(),
                12000,
                10000,
                0.000,
                0.00025,
                0.001,
                0.005,
            ),
            Market::new_stonk(
                6,
                StonkClass::Commodity,
                "Yuppies we are".into(),
                120000,
                7000,
                0.001,
                0.0025,
                0.001,
                0.15,
            ),
            Market::new_stonk(
                7,
                StonkClass::Commodity,
                "Tubbic".into(),
                12000,
                10000,
                0.001,
                0.0025,
                0.001,
                0.05,
            ),
        ];

        let mut m = Market {
            stonks,
            last_tick: 0,
            phase: GamePhase::Day {
                counter: DAY_LENGTH,
            },
            cycles: 0,
        };

        while m.cycles < HISTORICAL_SIZE / DAY_LENGTH {
            m.tick();
        }

        debug!("Starting market at {:?}", m.phase);

        m
    }

    pub fn new_stonk(
        id: usize,
        class: StonkClass,
        name: String,
        price_per_share_in_cents: u32,
        number_of_shares: u32,
        drift: f64,
        drift_volatility: f64,
        volatility: f64,
        shock_probability: f64,
    ) -> Stonk {
        Stonk::new(
            id,
            class,
            name,
            price_per_share_in_cents,
            number_of_shares,
            drift,
            drift_volatility,
            volatility,
            shock_probability,
        )
    }

    pub fn tick_day(&mut self) {
        let rng = &mut rand::thread_rng();
        let global_drift = if self.last_tick % DAY_LENGTH == 0 {
            Some(rng.gen_range(-0.01..0.01))
        } else {
            None
        };
        for stonk in self.stonks.iter_mut() {
            if let Some(drift) = global_drift {
                stonk.add_condition(
                    StonkCondition::GlobalDrift(drift),
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

    fn tick_night(&mut self) {
        // let rng = &mut rand::thread_rng();
    }
}

impl StonkMarket for Market {
    fn tick(&mut self) {
        debug!("\nMarket tick {:?}", self.phase);
        match self.phase {
            GamePhase::Day { counter } => {
                self.tick_day();
                if counter > 1 {
                    self.phase = GamePhase::Day {
                        counter: counter - 1,
                    }
                } else {
                    self.phase = GamePhase::Night {
                        counter: NIGHT_LENGTH,
                    }
                }
            }
            GamePhase::Night { counter } => {
                self.tick_night();
                if counter > 1 {
                    self.phase = GamePhase::Night {
                        counter: counter - 1,
                    };
                } else {
                    self.phase = GamePhase::Day {
                        counter: DAY_LENGTH,
                    };
                    self.cycles += 1;
                }
            }
        }
    }

    fn apply_agent_action<A: DecisionAgent>(&mut self, agent: &mut A) -> AppResult<()> {
        match self.phase {
            GamePhase::Day { .. } => match agent.selected_action() {
                Some(action) => match action {
                    AgentAction::Buy { stonk_id, amount } => {
                        let stonk = &mut self.stonks[stonk_id];
                        if stonk.number_of_shares == stonk.allocated_shares {
                            return Err("No more shares available".into());
                        }
                        let cost = stonk.buy_price() * amount;
                        agent.sub_cash(cost)?;
                        agent.add_stonk(stonk_id, amount)?;
                        stonk.allocated_shares += 1;
                        stonk.add_condition(StonkCondition::BumpUp, self.last_tick + 1);
                    }
                    AgentAction::Sell { stonk_id, amount } => {
                        let stonk = &mut self.stonks[stonk_id];
                        let cost = stonk.sell_price() * amount;
                        agent.sub_stonk(stonk_id, amount)?;
                        agent.add_cash(cost)?;
                        stonk.allocated_shares -= 1;
                        stonk.add_condition(StonkCondition::BumpDown, self.last_tick + 1);
                    }
                },
                None => {}
            },
            GamePhase::Night { .. } => {
                if agent.selected_action().is_some() {
                    return Err("No actions allowed during night".into());
                }
            }
        }

        agent.clear_action();

        Ok(())
    }
}

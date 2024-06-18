use crate::{
    agent::{AgentAction, DecisionAgent},
    utils::AppResult,
};
use rand::Rng;
use rand_distr::{Cauchy, Distribution, Normal};

const DAY_STARTING_HOUR: usize = 6;
const DAY_LENGTH_HOURS: usize = 16;
const NIGHT_LENGTH_HOURS: usize = 24 - DAY_LENGTH_HOURS;

// Each second represents 15 minutes => 1 hour = 4 ticks.
pub const DAY_LENGTH: usize = 4 * DAY_LENGTH_HOURS; // DAY_LENGTH = 16 hours
pub const NIGHT_LENGTH: usize = 4 * NIGHT_LENGTH_HOURS; // NIGHT_LENGTH = 8 hours

// We keep record of the last 8 weeks
pub const HISTORICAL_SIZE: usize = DAY_LENGTH * 7 * 8;

const MIN_DRIFT: f64 = -0.2;
const MAX_DRIFT: f64 = -MIN_DRIFT;

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
    pub stonks: [Stonk; 8],
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
                0.004,
                0.005,
                0.00075,
                0.02,
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

        println!("Starting market at {:?}", m.phase);

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
        Stonk {
            id,
            class,
            name,
            price_per_share_in_cents,
            number_of_shares,
            allocated_shares: 0,
            drift,
            drift_volatility,
            volatility: volatility.max(0.001).min(0.99),
            shock_probability,
            starting_price: price_per_share_in_cents,
            historical_prices: vec![],
            conditions: vec![],
        }
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
        println!("\nMarket tick {:?}", self.phase);
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

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StonkClass {
    Media,
    War,
    Commodity,
    Technology,
}

#[derive(Debug, Clone, Copy)]
pub enum StonkCondition {
    BumpUp,
    BumpDown,
    GlobalDrift(f64),
    NoShock(f64),
}

#[derive(Debug, Clone)]
pub struct Stonk {
    pub id: usize,
    pub class: StonkClass,
    pub name: String,
    pub price_per_share_in_cents: u32, //price is to be intended in cents, and displayed accordingly
    pub number_of_shares: u32,
    pub allocated_shares: u32,
    drift: f64,            // Cauchy dist mean, changes the mean price percentage variation
    drift_volatility: f64, // Influences the rate of change of drift, must be positive
    volatility: f64, // Cauchy dist variance, changes the variance of the price percentage variation, must be positive
    shock_probability: f64, // probability to select the Cauchy dist rather than the Guassian one
    starting_price: u32,
    pub historical_prices: Vec<u32>,
    conditions: Vec<(usize, StonkCondition)>,
}

impl Stonk {
    pub fn market_cap(&self) -> u32 {
        self.price_per_share_in_cents as u32 * self.number_of_shares as u32
    }

    pub fn apply_conditions(&mut self, current_tick: usize) {
        for (until_tick, condition) in self.conditions.iter() {
            match condition {
                StonkCondition::BumpUp => self.drift += self.drift_volatility,
                StonkCondition::BumpDown => self.drift -= self.drift_volatility,
                StonkCondition::GlobalDrift(drift) => self.drift += drift * self.drift_volatility,
                StonkCondition::NoShock(previous_shock_probability) => {
                    if *until_tick > current_tick {
                        self.shock_probability = 0.0
                    } else {
                        self.shock_probability = *previous_shock_probability
                    }
                }
            }
        }

        self.conditions
            .retain(|(until_tick, _)| *until_tick > current_tick);
    }

    pub fn add_condition(&mut self, condition: StonkCondition, until_tick: usize) {
        self.conditions.push((until_tick, condition));
    }

    pub fn tick(&mut self, current_tick: usize) {
        self.apply_conditions(current_tick);

        let rng = &mut rand::thread_rng();
        let price_drift = if rng.gen_bool(self.shock_probability) {
            Cauchy::new(self.drift, self.volatility)
                .expect("Failed to sample tick distribution")
                .sample(rng)
        } else {
            // self.price_per_share_in_cents = if rng.gen_bool((1.0 + self.drift) / 2.0) {
            //     self.buy_price().max(2)
            // } else {
            //     self.sell_price().max(1)
            // };
            Normal::new(self.drift, self.volatility)
                .expect("Failed to sample tick distribution")
                .sample(rng)
        }
        .min(MAX_DRIFT)
        .max(MIN_DRIFT);

        self.price_per_share_in_cents =
            (self.price_per_share_in_cents as f64 * (1.0 + price_drift)) as u32;

        self.historical_prices.push(self.price_per_share_in_cents);

        println!(
            "{:15} μ={:+.5} σ={:.5} Δ={:+.5} price={}\n{:?}",
            self.name,
            self.drift,
            self.volatility,
            price_drift,
            self.price_per_share_in_cents,
            self.conditions,
        );

        self.drift /= 2.0;
        if price_drift > 0.0 {
            if self.drift > 0.0 {
                self.add_condition(StonkCondition::BumpUp, current_tick + 1);
            } else {
                self.add_condition(StonkCondition::BumpUp, current_tick + 3);
            }
        } else if price_drift < 0.0 {
            if self.drift > 0.0 {
                self.add_condition(StonkCondition::BumpDown, current_tick + 3);
            } else {
                self.add_condition(StonkCondition::BumpDown, current_tick + 1);
            }
        }

        // self.drift = self.drift.min(MAX_DRIFT).max(MIN_DRIFT);

        // Add recovery mechanism for falling stonks. not ideal.
        if (self.price_per_share_in_cents as f64) < self.starting_price as f64 / 8.0 {
            self.add_condition(StonkCondition::BumpUp, current_tick + 1);
            self.add_condition(
                StonkCondition::NoShock(self.shock_probability),
                current_tick + 1,
            );
        } else if (self.price_per_share_in_cents as f64) > self.starting_price as f64 * 16.0 {
            self.add_condition(StonkCondition::BumpDown, current_tick + 1);
            self.add_condition(
                StonkCondition::NoShock(self.shock_probability),
                current_tick + 1,
            );
        }
    }

    fn modified_price(&self) -> f64 {
        // let modifier = (self.number_of_shares as f64
        //     / (self.number_of_shares - self.allocated_shares) as f64)
        //     .powf(0.25);
        self.price_per_share_in_cents as f64 //* modifier
    }

    fn buy_price(&self) -> u64 {
        (self.modified_price() * (1.0 + self.volatility)) as u64
    }

    fn sell_price(&self) -> u64 {
        (self.modified_price() * (1.0 - self.volatility)) as u64
    }

    pub fn formatted_buy_price(&self) -> f64 {
        self.buy_price() as f64 / 100.0
    }

    pub fn formatted_sell_price(&self) -> f64 {
        self.sell_price() as f64 / 100.0
    }
}

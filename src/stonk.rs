use crate::{
    agent::{AgentAction, DecisionAgent},
    utils::AppResult,
};
use crossterm::event::KeyCode;
use rand::Rng;

#[derive(Debug, Clone, Copy)]
pub enum GamePhase {
    Day { counter: u64 },
    Night { counter: u64 },
}

const PHASE_LENGTH: u64 = 240;
const HISTORICAL_SIZE: usize = 500;

pub trait StonkMarket {
    fn tick(&mut self);
    fn apply_agent_action<A: DecisionAgent>(&mut self, agent: &mut A) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub struct Market {
    pub stonks: [Stonk; 8],
    pub last_tick: u64,
    pub global_trend: f64,
    pub phase: GamePhase,
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
                0.005,
                0.015,
            ),
            Market::new_stonk(
                1,
                StonkClass::Technology,
                "Tesla".into(),
                10000,
                250,
                0.0,
                0.01,
            ),
            Market::new_stonk(
                2,
                StonkClass::Commodity,
                "Rovanti".into(),
                8000,
                250,
                0.005,
                0.005,
            ),
            Market::new_stonk(
                3,
                StonkClass::Media,
                "Riccardino".into(),
                9000,
                10000,
                0.000,
                0.0075,
            ),
            Market::new_stonk(
                4,
                StonkClass::War,
                "Mariottide".into(),
                80000,
                1000,
                0.000,
                0.001,
            ),
            Market::new_stonk(
                5,
                StonkClass::War,
                "Cubbit".into(),
                12000,
                10000,
                0.000,
                0.001,
            ),
            Market::new_stonk(
                6,
                StonkClass::Commodity,
                "Yuppies we are".into(),
                120000,
                7000,
                0.000,
                0.001,
            ),
            Market::new_stonk(
                7,
                StonkClass::Commodity,
                "Tubbic".into(),
                12000,
                10000,
                0.000,
                0.001,
            ),
        ];

        Market {
            stonks,
            last_tick: 1,
            global_trend: 0.0,
            phase: GamePhase::Day {
                counter: PHASE_LENGTH,
            },
        }
    }

    pub fn handle_key_events(&mut self, key_code: KeyCode) {
        match self.phase {
            GamePhase::Day { .. } => match key_code {
                KeyCode::Up => self.global_trend += 0.01,
                KeyCode::Down => self.global_trend -= 0.01,
                _ => {}
            },
            GamePhase::Night { .. } => {}
        }
    }

    pub fn new_stonk(
        id: usize,
        class: StonkClass,
        name: String,
        price_per_share_in_cents: u64,
        number_of_shares: u32,
        drift: f64,
        volatility: f64,
    ) -> Stonk {
        let mut s = Stonk {
            id,
            class,
            name,
            price_per_share_in_cents,
            number_of_shares,
            allocated_shares: 0,
            drift,
            drift_volatility: 0.00025,
            volatility: volatility.max(0.001).min(0.99),
            historical_prices: vec![price_per_share_in_cents],
        };
        for _ in 0..HISTORICAL_SIZE {
            s.tick();
        }

        s
    }

    pub fn x_ticks(&self) -> Vec<f64> {
        let min_tick = if self.last_tick > HISTORICAL_SIZE as u64 {
            self.last_tick - HISTORICAL_SIZE as u64
        } else {
            0
        };

        (min_tick..self.last_tick).map(|t| t as f64).collect()
    }

    pub fn tick_day(&mut self) {
        let rng = &mut rand::thread_rng();
        if self.last_tick % PHASE_LENGTH == 0 {
            self.global_trend = rng.gen_range(-0.02..0.02);
        }
        for stonk in self.stonks.iter_mut() {
            stonk.drift += (self.global_trend - stonk.drift) * rng.gen_range(0.25..0.75);
            stonk.tick();
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
        match self.phase {
            GamePhase::Day { counter } => {
                self.tick_day();
                if counter > 0 {
                    self.phase = GamePhase::Day {
                        counter: counter - 1,
                    }
                } else {
                    self.phase = GamePhase::Night {
                        counter: PHASE_LENGTH / 10,
                    }
                }
            }
            GamePhase::Night { counter } => {
                self.tick_night();
                if counter > 0 {
                    self.phase = GamePhase::Night {
                        counter: counter - 1,
                    }
                } else {
                    self.phase = GamePhase::Day {
                        counter: PHASE_LENGTH,
                    }
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
                        stonk.drift = (stonk.drift + stonk.drift_volatility).min(1.0);
                    }
                    AgentAction::Sell { stonk_id, amount } => {
                        let stonk = &mut self.stonks[stonk_id];
                        let cost = stonk.sell_price() * amount;
                        agent.sub_stonk(stonk_id, amount)?;
                        agent.add_cash(cost)?;
                        stonk.allocated_shares -= 1;
                        stonk.drift = (stonk.drift - stonk.drift_volatility).max(-1.0);
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

#[derive(Debug, Clone)]
pub enum StonkClass {
    Media,
    War,
    Commodity,
    Technology,
}

#[derive(Debug, Clone)]
pub struct Stonk {
    pub id: usize,
    pub class: StonkClass,
    pub name: String,
    pub price_per_share_in_cents: u64, //price is to be intended in cents, and displayed accordingly
    pub number_of_shares: u32,
    pub allocated_shares: u32,
    pub drift: f64,
    pub drift_volatility: f64,
    pub volatility: f64,
    pub historical_prices: Vec<u64>,
}

impl Stonk {
    pub fn data(&self, x_ticks: Vec<f64>) -> Vec<(f64, f64)> {
        if self.historical_prices.len() < x_ticks.len() {
            return vec![];
        }
        x_ticks
            .iter()
            .enumerate()
            .map(|(idx, t)| {
                (
                    *t,
                    self.historical_prices[self.historical_prices.len() + idx - x_ticks.len()]
                        as f64
                        / 100.0,
                )
            })
            .collect::<Vec<(f64, f64)>>()
    }

    pub fn market_cap(&self) -> u32 {
        self.price_per_share_in_cents as u32 * self.number_of_shares as u32
    }

    pub fn tick(&mut self) {
        let rng = &mut rand::thread_rng();
        self.price_per_share_in_cents = if rng.gen_bool((1.0 + self.drift) / 2.0) {
            self.buy_price().max(2)
        } else {
            self.sell_price().max(1)
        };
        self.historical_prices.push(self.price_per_share_in_cents);
    }

    fn modified_price(&self) -> f64 {
        let modifier = (self.number_of_shares as f64
            / (self.number_of_shares - self.allocated_shares) as f64)
            .powf(0.25);
        self.price_per_share_in_cents as f64 * modifier
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

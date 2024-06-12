use crossterm::event::KeyCode;
use rand::Rng;
use ratatui::style::{Style, Stylize};
use std::collections::HashMap;

use crate::{
    agent::{AgentAction, DecisionAgent},
    utils::AppResult,
};

#[derive(Debug, Clone, Copy)]
pub enum GamePhase {
    Day { counter: u64 },
    Night { counter: u64 },
}

const PHASE_LENGTH: u64 = 120;

pub trait StonkMarket {
    fn tick(&mut self);
    fn apply_agent_action<A: DecisionAgent>(&mut self, agent: &mut A) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub struct Market {
    pub stonks: HashMap<usize, Stonk>,
    pub last_tick: u64,
    pub historical_size: usize,
    pub global_trend: f64,
    pub phase: GamePhase,
}

impl Market {
    pub fn new() -> Self {
        Market {
            stonks: HashMap::new(),
            last_tick: 1,
            historical_size: 500,
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
        &mut self,
        class: StonkClass,
        name: String,
        price_per_share: f64,
        number_of_shares: u32,
        drift: f64,
        volatility: f64,
    ) {
        let mut s = Stonk {
            id: self.stonks.len(),
            class,
            name,
            price_per_share,
            number_of_shares,
            allocated_shares: 0,
            drift,
            drift_volatility: 0.005,
            volatility: volatility.max(0.001).min(0.99),
            historical_prices: vec![price_per_share],
        };
        for _ in 0..self.historical_size {
            s.tick();
        }
        self.stonks.insert(s.id, s);
    }

    pub fn tick_day(&mut self) {
        let rng = &mut rand::thread_rng();
        if self.last_tick % PHASE_LENGTH == 0 {
            self.global_trend = rng.gen_range(-0.2..0.2);
        }
        for (_, stonk) in self.stonks.iter_mut() {
            stonk.drift += (self.global_trend - stonk.drift) * rng.gen_range(0.25..0.75);
            stonk.tick();
            while stonk.historical_prices.len() > self.historical_size {
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
                        counter: PHASE_LENGTH / 2,
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
                        if let Some(stonk) = self.stonks.get_mut(&stonk_id) {
                            if stonk.number_of_shares == stonk.allocated_shares {
                                return Err("No more shares available".into());
                            }
                            let cost = stonk.buy_price() * amount as f64;
                            agent.sub_cash(cost)?;
                            agent.add_stonk(stonk_id, amount)?;
                            stonk.allocated_shares += 1;
                            stonk.drift = (stonk.drift + stonk.drift_volatility).min(1.0);
                        }
                    }
                    AgentAction::Sell { stonk_id, amount } => {
                        if let Some(stonk) = self.stonks.get_mut(&stonk_id) {
                            let cost = stonk.sell_price() * amount as f64;
                            agent.sub_stonk(stonk_id, amount)?;
                            agent.add_cash(cost)?;
                            stonk.allocated_shares -= 1;
                            stonk.drift = (stonk.drift - stonk.drift_volatility).max(-1.0);
                        }
                    }
                },
                None => {}
            },
            GamePhase::Night { .. } => {
                return Err("No actions allowed during noght".into());
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

impl StonkClass {
    pub fn style(&self) -> Style {
        match self {
            StonkClass::Media => Style::default().cyan(),
            StonkClass::War => Style::default().red(),
            StonkClass::Commodity => Style::default().magenta(),
            StonkClass::Technology => Style::default().green(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stonk {
    pub id: usize,
    pub class: StonkClass,
    pub name: String,
    pub price_per_share: f64,
    pub number_of_shares: u32,
    pub allocated_shares: u32,
    pub drift: f64,
    pub drift_volatility: f64,
    pub volatility: f64,
    pub historical_prices: Vec<f64>,
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
                    self.historical_prices[self.historical_prices.len() + idx - x_ticks.len()],
                )
            })
            .collect::<Vec<(f64, f64)>>()
    }

    pub fn market_cap(&self) -> u32 {
        self.price_per_share as u32 * self.number_of_shares as u32
    }

    pub fn tick(&mut self) {
        let rng = &mut rand::thread_rng();
        self.price_per_share = if rng.gen_bool((1.0 + self.drift) / 2.0) {
            self.price_per_share * (1.0 + self.volatility)
        } else {
            self.price_per_share * (1.0 - self.volatility)
        };
        self.historical_prices.push(self.price_per_share);
    }

    fn modified_price(&self) -> f64 {
        let modifier = (self.number_of_shares as f64
            / (self.number_of_shares - self.allocated_shares) as f64)
            .powf(0.25);
        self.price_per_share * modifier as f64
    }

    pub fn buy_price(&self) -> f64 {
        self.modified_price() * (1.0 + self.volatility)
    }

    pub fn sell_price(&self) -> f64 {
        self.modified_price() * (1.0 - self.volatility)
    }
}

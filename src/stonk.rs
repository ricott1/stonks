use rand::Rng;
use ratatui::style::{Style, Stylize};

#[derive(Debug, Clone, Copy)]
pub enum GamePhase {
    Day { counter: usize },
    Night { counter: usize },
}

const PHASE_LENGTH: usize = 120;

#[derive(Debug, Clone)]
pub struct App {
    pub stonks: Vec<Stonk>,
    pub last_tick: u64,
    pub historical_size: usize,
    pub global_trend: f64,
    pub phase: GamePhase,
}

impl App {
    pub fn new() -> Self {
        App {
            stonks: vec![],
            last_tick: 1,
            historical_size: 500,
            global_trend: 0.0,
            phase: GamePhase::Day {
                counter: PHASE_LENGTH,
            },
        }
    }

    pub fn new_stonk(
        &mut self,
        class: StonkClass,
        name: String,
        price_per_share: f64,
        number_of_shares: u16,
        drift: f64,
        volatility: f64,
    ) {
        let s = Stonk {
            id: self.stonks.len(),
            class,
            name,
            price_per_share,
            number_of_shares,
            drift,
            volatility: volatility.max(0.001).min(0.99),
            historical_prices: vec![price_per_share],
            average: price_per_share,
        };
        self.stonks.push(s);
    }

    pub fn tick_day(&mut self) {
        let rng = &mut rand::thread_rng();
        if self.last_tick % 120 == 0 {
            self.global_trend = rng.gen_range(-0.2..0.2);
        }
        for stonk in self.stonks.iter_mut() {
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

    pub fn tick(&mut self) {
        match self.phase {
            GamePhase::Day { counter } => {
                self.tick_day();
                if counter > 0 {
                    self.phase = GamePhase::Day {
                        counter: counter - 1,
                    }
                } else {
                    self.phase = GamePhase::Night {
                        counter: PHASE_LENGTH,
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

    pub fn x_ticks(&self) -> Vec<f64> {
        let min_tick = if self.last_tick > self.historical_size as u64 {
            self.last_tick - self.historical_size as u64
        } else {
            0
        };

        (min_tick..self.last_tick).map(|t| t as f64).collect()
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
    pub number_of_shares: u16,
    pub drift: f64,
    pub volatility: f64,
    pub historical_prices: Vec<f64>,
    pub average: f64,
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

    pub fn avg_data(&self, x_ticks: Vec<f64>) -> Vec<(f64, f64)> {
        x_ticks
            .iter()
            .map(|t| (*t, self.average))
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

        // let avg_size = 100;
        // let min = if self.historical_prices.len() > avg_size {
        //     self.historical_prices.len() - avg_size
        // } else {
        //     0
        // };
        // self.average = self.historical_prices.iter().skip(min).sum::<f64>()
        //     / avg_size.min(self.historical_prices.len()) as f64;
    }
}

#[derive(Debug, Clone)]
enum AgentAction {
    Buy { id: u64 },
    Sell { id: u64 },
}

trait DecisionAgent {
    fn cash(&self) -> u32;
    fn actions(&self) -> Vec<AgentAction>;
}

use crate::utils::AppResult;
use rand::Rng;
use rand_distr::{Cauchy, Distribution, Normal};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

const MIN_DRIFT: f64 = -0.2;
const MAX_DRIFT: f64 = -MIN_DRIFT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StonkClass {
    Media,
    War,
    Commodity,
    Technology,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StonkCondition {
    Bump {
        amount: f64,
    },
    SetShockProbability {
        value: f64,
        previous_shock_probability: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stonk {
    pub id: usize,
    pub class: StonkClass,
    pub name: String,
    pub short_name: String,
    pub description: String,
    pub price_per_share_in_cents: u32, //price is to be intended in cents, and displayed accordingly
    pub number_of_shares: u32,
    pub allocated_shares: u32,
    pub shareholders: Vec<(String, u32)>, // List of shareholders, always sorted from biggest to smallest.
    drift: f64,            // Cauchy dist mean, changes the mean price percentage variation
    drift_volatility: f64, // Influences the rate of change of drift, must be positive
    volatility: f64, // Cauchy dist variance, changes the variance of the price percentage variation, must be positive
    pub shock_probability: f64, // probability to select the Cauchy dist rather than the Guassian one
    starting_price: u32,
    pub historical_prices: Vec<u32>,
    conditions: Vec<(usize, StonkCondition)>,
}

impl Stonk {
    fn sort_shareholders(&mut self) {
        self.shareholders.sort_by(|(_, a), (_, b)| b.cmp(a))
    }

    pub fn to_stake(&self, amount: u32) -> f64 {
        amount as f64 / self.number_of_shares as f64
    }

    pub fn info(&self, amount: u32) -> String {
        let share = self.to_stake(amount) * 100.0;
        if share >= 5.0 {
            format!(
                "Price ${:.2} - Drift {:.3} - Volatility {:.3}",
                self.price_per_share_in_cents as f64 / 100.0,
                self.drift,
                self.volatility
            )
        } else if share >= 1.0 {
            format!(
                "Price ${:.2} - Drift {:.3}",
                self.price_per_share_in_cents as f64 / 100.0,
                self.drift
            )
        } else {
            format!("Price ${:.2}", self.price_per_share_in_cents as f64 / 100.0)
        }
    }

    pub fn market_cap(&self) -> u32 {
        self.price_per_share_in_cents as u32 * self.number_of_shares as u32
    }

    pub fn available_amount(&self) -> u32 {
        self.number_of_shares - self.allocated_shares
    }

    pub fn allocate_shares(&mut self, username: &str, amount: u32) -> AppResult<()> {
        if amount > self.available_amount() {
            return Err("Amount is greater than number of available shares.".into());
        }
        if let Some((_, old_amount)) = self
            .shareholders
            .iter_mut()
            .find(|(holder, _)| *holder == username.to_string())
        {
            *old_amount += amount
        } else {
            self.shareholders.push((username.to_string(), amount))
        }
        self.sort_shareholders();

        info!("New shareholders: {:#?}", self.shareholders);
        Ok(())
    }

    pub fn deallocate_shares(&mut self, username: &str, amount: u32) -> AppResult<()> {
        if let Some((_, old_amount)) = self
            .shareholders
            .iter_mut()
            .find(|(holder, _)| *holder == username.to_string())
        {
            if amount > *old_amount {
                return Err("Amount is greater than number of shares owned by agent.".into());
            }
            *old_amount -= amount
        } else {
            return Err("Agent is not a shareholder".into());
        }
        self.sort_shareholders();
        info!("New shareholders: {:#?}", self.shareholders);

        Ok(())
    }

    pub fn apply_conditions(&mut self, current_tick: usize) {
        for (until_tick, condition) in self.conditions.iter() {
            match condition {
                StonkCondition::Bump { amount } => self.drift += amount * self.drift_volatility,
                StonkCondition::SetShockProbability {
                    value,
                    previous_shock_probability,
                } => {
                    if *until_tick > current_tick {
                        self.shock_probability = *value
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

        self.price_per_share_in_cents = ((self.price_per_share_in_cents as f64
            * (1.0 + price_drift)) as u32)
            .max(self.starting_price / 100); // Cannot go below one hundreth of starting price

        self.historical_prices.push(self.price_per_share_in_cents);

        debug!(
            "{:15} μ={:+.5} σ={:.5} Δ={:+.5} shock={:.3} price={}\n{:?}",
            self.name,
            self.drift,
            self.volatility,
            price_drift,
            self.shock_probability,
            self.price_per_share_in_cents,
            self.conditions,
        );

        self.drift /= 2.0;
        if price_drift > 0.0 {
            if self.drift > 0.0 {
                self.add_condition(StonkCondition::Bump { amount: 1.0 }, current_tick + 1);
            } else {
                self.add_condition(StonkCondition::Bump { amount: 2.5 }, current_tick + 3);
            }
        } else if price_drift < 0.0 {
            if self.drift > 0.0 {
                self.add_condition(StonkCondition::Bump { amount: -1.0 }, current_tick + 3);
            } else {
                self.add_condition(StonkCondition::Bump { amount: -2.5 }, current_tick + 1);
            }
        }

        // self.drift = self.drift.min(MAX_DRIFT).max(MIN_DRIFT);

        // Add control mechanisms for extreme prices. not ideal.
        if (self.price_per_share_in_cents as f64) < self.starting_price as f64 / 8.0 {
            self.add_condition(StonkCondition::Bump { amount: 2.5 }, current_tick + 1);
            self.add_condition(
                StonkCondition::SetShockProbability {
                    value: 0.0,
                    previous_shock_probability: self.shock_probability,
                },
                current_tick + 1,
            );
        } else if (self.price_per_share_in_cents as f64) > self.starting_price as f64 * 16.0 {
            self.add_condition(StonkCondition::Bump { amount: -2.5 }, current_tick + 1);
            self.add_condition(
                StonkCondition::SetShockProbability {
                    value: 0.0,
                    previous_shock_probability: self.shock_probability,
                },
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

    pub fn buy_price(&self) -> u32 {
        (self.modified_price() * (1.0 + self.volatility)) as u32
    }

    pub fn sell_price(&self) -> u32 {
        (self.modified_price() * (1.0 - self.volatility)) as u32
    }

    pub fn buy_price_dollars(&self) -> f64 {
        self.buy_price() as f64 / 100.0
    }

    pub fn sell_price_dollars(&self) -> f64 {
        self.sell_price() as f64 / 100.0
    }

    pub fn market_cap_dollars(&self) -> f64 {
        self.market_cap() as f64 / 100.0
    }
}

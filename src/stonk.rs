use crate::utils::AppResult;
use rand::Rng;
use rand_distr::{Cauchy, Distribution, Normal};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

const MAX_PRICE_DRIFT: f64 = 0.2;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StonkClass {
    #[default]
    Media,
    War,
    Commodity,
    Technology,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StonkCondition {
    Bump { amount: f64 },
    IncreasedShockProbability,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Stonk {
    pub id: usize,
    pub class: StonkClass,
    pub name: String,
    pub short_name: String,
    pub description: String,
    price_per_share_in_cents: u32, //price is to be intended in cents, and displayed accordingly
    pub number_of_shares: u32,
    pub allocated_shares: u32,
    pub shareholders: Vec<(String, u32)>, // List of shareholders, always sorted from biggest to smallest.
    drift: f64,            // Cauchy dist mean, changes the mean price percentage variation
    drift_volatility: f64, // Influences the rate of change of drift, must be positive
    volatility: f64, // Cauchy dist variance, changes the variance of the price percentage variation, must be positive
    pub shock_probability: f64, // probability to select the Cauchy dist rather than the Guassian one
    pub starting_price: u32,
    pub historical_prices: Vec<u32>,
    conditions: Vec<(usize, StonkCondition)>,
}

impl Stonk {
    fn sort_shareholders(&mut self) {
        self.shareholders.retain(|(_, amount)| *amount > 0);
        self.shareholders.sort_by(|(_, a), (_, b)| b.cmp(a));
    }

    #[allow(dead_code)]
    pub(crate) fn set_test_values(
        &mut self,
        price_per_share_in_cents: u32,
        number_of_shares: u32,
        drift: f64,
        drift_volatility: f64,
        volatility: f64,
        shock_probability: f64,
    ) {
        self.price_per_share_in_cents = price_per_share_in_cents;
        self.starting_price = self.price_per_share_in_cents;
        self.number_of_shares = number_of_shares;
        self.drift = drift;
        self.drift_volatility = drift_volatility;
        self.volatility = volatility;
        self.shock_probability = shock_probability;
    }

    pub fn to_stake(&self, amount: u32) -> f64 {
        amount as f64 / self.number_of_shares as f64
    }

    pub fn info(&self, amount: u32) -> String {
        let share = self.to_stake(amount) * 100.0;
        if share >= 5.0 {
            format!(
                "Price ${:.02} - Drift {:.03}% - Volatility {:.03}%",
                self.price_per_share_in_cents as f64 / 100.0,
                self.drift * 100.0,
                self.volatility * 100.0
            )
        } else if share >= 1.0 {
            format!(
                "Price ${:.02} - Drift {:.03}%",
                self.price_per_share_in_cents as f64 / 100.0,
                self.drift * 100.0
            )
        } else {
            format!(
                "Price ${:.02}",
                self.price_per_share_in_cents as f64 / 100.0
            )
        }
    }

    pub fn market_cap_cents(&self) -> u64 {
        self.price_per_share_in_cents as u64 * self.number_of_shares as u64
    }

    pub fn available_amount(&self) -> u32 {
        self.number_of_shares - self.allocated_shares
    }

    fn allocate_shares(&mut self, amount: u32) -> AppResult<()> {
        if amount > self.available_amount() {
            return Err("Amount is greater than number of available shares.".into());
        }

        if amount == 0 {
            return Ok(());
        }

        self.allocated_shares += amount;
        Ok(())
    }

    pub fn allocate_shares_to_agent(&mut self, username: &str, amount: u32) -> AppResult<()> {
        self.allocate_shares(amount)?;

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

    fn deallocate_shares(&mut self, amount: u32) -> AppResult<()> {
        if amount > self.allocated_shares {
            return Err("Amount is greater than number of allocated shares.".into());
        }

        if amount == 0 {
            return Ok(());
        }

        self.allocated_shares -= amount;
        Ok(())
    }

    pub fn deallocate_shares_to_agent(&mut self, username: &str, amount: u32) -> AppResult<()> {
        self.deallocate_shares(amount)?;

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

    fn apply_conditions(&mut self, current_tick: usize) {
        for (_, condition) in self.conditions.iter() {
            match condition {
                StonkCondition::Bump { amount } => self.drift += amount * self.drift_volatility,
                StonkCondition::IncreasedShockProbability => {
                    // This condition is checked during the stonk tick
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
        let shock_probability = if self
            .conditions
            .iter()
            .any(|(_, condition)| *condition == StonkCondition::IncreasedShockProbability)
        {
            (2.0 * self.shock_probability).min(0.2)
        } else {
            self.shock_probability
        };
        let price_drift = if rng.gen_bool(shock_probability) {
            Cauchy::new(self.drift, self.volatility)
                .expect("Failed to sample tick distribution")
                .sample(rng)
        } else {
            self.drift
                + self.volatility
                    * Normal::new(0.0, 1.0)
                        .expect("Failed to sample tick distribution")
                        .sample(rng)
        }
        .min(MAX_PRICE_DRIFT)
        .max(-MAX_PRICE_DRIFT);

        self.price_per_share_in_cents = ((self.price_per_share_in_cents as f64
            * (1.0 + price_drift)) as u32)
            .max(self.starting_price / 100); // Cannot go below one hundreth of starting price

        self.historical_prices.push(self.price_per_share_in_cents);

        debug!(
            "{:15} μ={:+.5} σ={:.5} Δ={:+.5} shock={:.03} price={}\n{:?}",
            self.name,
            self.drift,
            self.volatility,
            price_drift,
            self.shock_probability,
            self.price_per_share_in_cents,
            self.conditions,
        );

        self.drift /= 2.0;
        self.add_condition(
            StonkCondition::Bump {
                amount: price_drift,
            },
            current_tick + 1,
        );

        // Add control mechanisms for extreme prices. not ideal.
        if (self.price_per_share_in_cents as f64) < self.starting_price as f64 / 8.0 {
            self.add_condition(StonkCondition::Bump { amount: 1.0 }, current_tick + 1);
            self.add_condition(StonkCondition::IncreasedShockProbability, current_tick + 1);
        } else if (self.price_per_share_in_cents as f64) > self.starting_price as f64 * 8.0 {
            self.add_condition(StonkCondition::Bump { amount: -1.0 }, current_tick + 1);
            self.add_condition(StonkCondition::IncreasedShockProbability, current_tick + 1);
        }
    }

    fn base_price(&self) -> u32 {
        // let mut price = 0;
        // for l in 0..amount {
        //     let current_available_amount = self.available_amount() - l;
        //     let modifier = ((self.number_of_shares + MODIFIED_PRICE_DELTA) as f64
        //         / (current_available_amount + MODIFIED_PRICE_DELTA) as f64)
        //         .powf(0.5);

        //     price += (self.price_per_share_in_cents as f64 * modifier) as u32
        // }
        self.price_per_share_in_cents
    }

    fn buy_price(&self, amount: u32) -> u32 {
        // The price to buy the first share is base_price * ( 1.0 + volatility ).
        // Each subsequent share adds one unit of volatility
        // ( 1.0 + 2.0*volatility ) , ( 1.0 + 3.0*volatility ) ....
        // so that the total price is just the summation
        // giving base_price * amount * ( 1.0 + (amount + 1.0) / 2.0 * volatility )
        ((self.base_price() * amount) as f64 * (1.0 + (amount + 1) as f64 / 2.0 * self.volatility))
            as u32
    }

    fn sell_price(&self, amount: u32) -> u32 {
        // The price to sell the first share is base_price * ( 1.0 - volatility ).
        // Each subsequent share adds one unit of volatility
        // ( 1.0 - 2.0*volatility ) , ( 1.0 - 3.0*volatility ) ....
        // so that the total price is just the summation
        // giving base_price * amount * ( 1.0 - (amount + 1.0) / 2.0 * volatility )
        // Notice that the volatility is then contrained by
        // 1 - number_of_shares * volatility >= 0 ==> volatility <= 1/number_of_shares
        ((self.base_price() * amount) as f64
            * (1.0
                - (amount + 1) as f64 / 2.0
                    * self.volatility.min(1.0 / self.number_of_shares as f64))) as u32
    }

    fn current_price(&self) -> u32 {
        self.base_price()
    }

    pub fn buy_price_cents(&self, amount: u32) -> u32 {
        self.buy_price(amount)
    }

    pub fn sell_price_cents(&self, amount: u32) -> u32 {
        self.sell_price(amount)
    }

    pub fn current_unit_price_cents(&self) -> u32 {
        self.current_price()
    }

    pub fn max_buy_amount(&self, cash: u32) -> u32 {
        // We need to solve cash == buy_price(amount) for amount
        // and then take the floor of amount
        // cash == base_price * amount * (1.0 + (amount + 1) / 2.0 * volatility)
        let max_amount = (-(2.0 + self.volatility)
            + (8.0 * cash as f64 * self.volatility / self.base_price() as f64
                + (2.0 + self.volatility).powf(2.0))
            .powf(0.5))
            / (2.0 * self.volatility);
        max_amount as u32
    }
}

pub trait DollarValue {
    fn as_dollars(&self) -> f64;
    fn format(&self) -> String {
        let value = self.as_dollars();
        if value > 1_000_000.0 {
            format!("{:.03}M", value / 1_000_000.0)
        } else if value > 1_000.0 {
            format!("{:.03}k", value / 1_000.0)
        } else {
            format!("{:.02}", value)
        }
    }
}

impl DollarValue for u32 {
    fn as_dollars(&self) -> f64 {
        *self as f64 / 100.0
    }
}

impl DollarValue for u64 {
    fn as_dollars(&self) -> f64 {
        *self as f64 / 100.0
    }
}

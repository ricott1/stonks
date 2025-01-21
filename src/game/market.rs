use std::collections::{HashMap, HashSet};

use super::{
    agent::{AgentAction, AgentCondition, DecisionAgent, UserAgent, INITIAL_USER_CASH_CENTS},
    events::{CHARACTER_ASSASSINATION_COST, DIVIDEND_PAYOUT, MARKET_CRASH_COST},
    stonk::{DollarValue, Stonk, StonkCondition},
};
use crate::{
    game::events::NightEvent,
    utils::{load_stonks_data, AgentId, AppResult},
};
use anyhow::anyhow;
use log::{debug, info};
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, IntoEnumIterator};

const DAY_STARTING_HOUR: usize = 6;
const DAY_LENGTH_HOURS: usize = 18;
const NIGHT_LENGTH_HOURS: usize = 24 - DAY_LENGTH_HOURS;

const TICKS_PER_HOUR: usize = 4;
pub const MAX_EVENTS_PER_NIGHT: usize = 3;

// Each second represents 15 minutes => 1 hour = 4 ticks.
pub const DAY_LENGTH: usize = TICKS_PER_HOUR * DAY_LENGTH_HOURS; // DAY_LENGTH = 16 hours
pub const NIGHT_LENGTH: usize = TICKS_PER_HOUR * NIGHT_LENGTH_HOURS; // NIGHT_LENGTH = 8 hours

// We keep record of the last 12 weeks
pub const HISTORICAL_SIZE: usize = DAY_LENGTH * 30 * 12;
pub const NUMBER_OF_STONKS: usize = 8;

const BRIBE_AMOUNT: u32 = 10_000 * 100;

const MAX_GLOBAL_DRIFT: f64 = 0.25;
const GLOBAL_DRIFT_VOLATILITY: f64 = 0.05;
const GLOBAL_DRIFT_INTERVAL: usize = DAY_LENGTH;

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
    pub agents: HashMap<AgentId, UserAgent>,
    #[serde(skip)]
    pub online_agents: HashSet<AgentId>,
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
            agents: HashMap::new(),
            online_agents: HashSet::new(),
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
            "Current total market cap: ${}",
            m.total_market_cap().format()
        );

        m
    }

    pub fn initialize(&mut self, rng: &mut ChaCha8Rng) {
        loop {
            self.tick_day(rng);
            if self.last_tick == HISTORICAL_SIZE / 4 {
                break;
            }
        }
    }

    pub fn get_online_agent(&mut self, id: &AgentId) -> Option<&mut UserAgent> {
        if self.online_agents.contains(id) {
            self.agents.get_mut(id)
        } else {
            None
        }
    }

    pub fn add_online_agent(&mut self, id: AgentId, agent: UserAgent) {
        self.agents.insert(id, agent);
        self.online_agents.insert(id);
    }

    pub fn remove_online_agent(&mut self, id: AgentId) {
        self.online_agents.remove(&id);
    }

    pub fn total_market_cap(&self) -> u64 {
        self.stonks
            .iter()
            .map(|stonk| stonk.market_cap_cents() as u64)
            .sum::<u64>()
    }

    pub fn update_target_total_market_cap(&mut self) -> u64 {
        self.target_total_market_cap = self.initial_total_market_cap
            + self.agents.len() as u64 * INITIAL_USER_CASH_CENTS as u64;
        self.target_total_market_cap
    }

    pub fn update_portfolios(&mut self) {
        let mut portfolios = vec![];
        for (_, agent) in self.agents.iter() {
            let agent_value = agent
                .owned_stonks()
                .iter()
                .enumerate()
                .map(|(stonk_id, amount)| {
                    let stonk = &self.stonks[stonk_id];
                    stonk.current_unit_price_cents() as u64 * *amount as u64
                })
                .sum::<u64>()
                + agent.cash() as u64;
            if agent_value > 0 {
                portfolios.push((agent.username().to_string(), agent_value));
            }
        }

        portfolios.sort_by(|(_, a), (_, b)| b.cmp(a));

        self.portfolios = portfolios;
    }

    pub fn tick_day(&mut self, rng: &mut ChaCha8Rng) {
        let global_drift = if self.last_tick % GLOBAL_DRIFT_INTERVAL == 0 {
            let current_market_cap = self.total_market_cap() as f64;
            let mean = (self.target_total_market_cap as f64 - current_market_cap)
                / current_market_cap.min(self.target_total_market_cap as f64);
            let drift = (mean + rng.gen_range(-GLOBAL_DRIFT_VOLATILITY..GLOBAL_DRIFT_VOLATILITY))
                .min(MAX_GLOBAL_DRIFT)
                .max(-MAX_GLOBAL_DRIFT);

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
                    self.last_tick + GLOBAL_DRIFT_INTERVAL,
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

    pub fn tick(&mut self) -> AppResult<()> {
        debug!("\nMarket tick {:?}", self.phase);
        self.update_target_total_market_cap();
        self.update_portfolios();

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

        self.tick_agents()?;

        Ok(())
    }

    fn tick_agents(&mut self) -> AppResult<()> {
        // Prepare CharacterAssassination events common to every agent
        let mut character_assassination_events = vec![];
        let mut usernames = vec![];
        for stonk in self.stonks.iter() {
            for (username, _) in stonk.shareholders.iter().take(5) {
                if usernames.contains(&username) {
                    continue;
                }

                if let Some(agent) = self.agents.get(username) {
                    if agent
                        .past_selected_actions()
                        .contains_key(&AgentAction::AcceptBribe.to_string())
                        && !agent
                            .past_selected_actions()
                            .contains_key(&AgentAction::AssassinationVictim.to_string())
                    {
                        usernames.push(username);
                        character_assassination_events.push(NightEvent::CharacterAssassination {
                            agent_id: agent.id(),
                            username: username.to_string(),
                        })
                    }
                }
            }
        }
        // Apply agent actions and set night events

        // CrashAgentStonks actions are handled first
        let crash_agent_targets = self
            .agents
            .iter()
            .map(|(_, agent)| {
                if let Some(action) = agent.selected_action() {
                    if let AgentAction::CrashAgentStonks { agent_id } = action {
                        Some(agent_id)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .filter(|a| a.is_some())
            .map(|a| a.copied().unwrap())
            .collect::<Vec<AgentId>>();

        for target_id in crash_agent_targets {
            if let Some(target) = self.agents.get_mut(&target_id) {
                target
                    .insert_past_selected_actions(AgentAction::AssassinationVictim, self.last_tick);

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
                        StonkCondition::IncreasedShockProbability,
                        self.last_tick + DAY_LENGTH,
                    );
                }
            }
        }

        for agent_id in self.online_agents.iter() {
            let agent = if let Some(agent) = self.agents.get_mut(agent_id) {
                agent
            } else {
                continue;
            };

            agent.apply_conditions(self.last_tick);

            match self.phase {
                GamePhase::Day { .. } => {
                    let agent_action = agent.selected_action().copied();
                    agent.clear_render_counter();
                    agent.set_available_night_events(vec![]);
                    if let Some(action) = agent_action {
                        agent.clear_action();
                        agent.insert_past_selected_actions(action, self.last_tick);

                        match action {
                            AgentAction::Buy { stonk_id, amount } => {
                                let stonk = &mut self.stonks[stonk_id];
                                let max_amount = stonk.available_amount();
                                if max_amount < amount {
                                    return Err(anyhow!("Not enough shares available"));
                                }

                                let cost = stonk.buy_price_cents(amount);
                                agent.sub_cash(cost)?;

                                agent.add_stonk(stonk_id, amount)?;
                                stonk.allocate_shares_to_agent(agent.id(), amount)?;

                                info!(
                                "{} stonks bought, there are now {} available ({} total bought)",
                                amount,
                                stonk.available_amount(),
                                stonk.allocated_shares
                            );

                                let bump_amount = stonk.to_stake(amount) * 100.0;
                                stonk.add_condition(
                                    StonkCondition::Bump {
                                        amount: bump_amount,
                                    },
                                    self.last_tick + 1,
                                );
                            }
                            AgentAction::Sell { stonk_id, amount } => {
                                let stonk = &mut self.stonks[stonk_id];

                                let cost = stonk.sell_price_cents(amount);
                                agent.add_cash(cost)?;
                                agent.sub_stonk(stonk_id, amount)?;
                                stonk.deallocate_shares_to_agent(agent.id(), amount)?;

                                info!(
                                    "{} stonks sold, there are now {} available ({} total bought)",
                                    amount,
                                    stonk.available_amount(),
                                    stonk.allocated_shares
                                );

                                let bump_amount = stonk.to_stake(amount) * 100.0;
                                stonk.add_condition(
                                    StonkCondition::Bump {
                                        amount: -bump_amount,
                                    },
                                    self.last_tick + 1,
                                );
                            }
                            AgentAction::BumpStonkClass { class } => {
                                for stonk in self.stonks.iter_mut().filter(|s| s.class == class) {
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
                                        StonkCondition::IncreasedShockProbability,
                                        self.last_tick + DAY_LENGTH,
                                    )
                                }
                                agent.sub_cash(MARKET_CRASH_COST)?;
                            }
                            AgentAction::AddCash { amount } => {
                                agent.add_cash(amount)?;
                            }

                            AgentAction::AcceptBribe => {
                                agent.add_cash(BRIBE_AMOUNT)?;
                            }

                            AgentAction::OneDayUltraVision => agent.add_condition(
                                AgentCondition::UltraVision,
                                self.last_tick + DAY_LENGTH,
                            ),
                            AgentAction::CrashAgentStonks { .. } => {
                                agent.sub_cash(CHARACTER_ASSASSINATION_COST)?;
                            }
                            AgentAction::AssassinationVictim => {}
                            AgentAction::GetDividends { stonk_id } => {
                                let stonk = &self.stonks[stonk_id];
                                let yesterday_opening_price = stonk.historical_prices
                                    [stonk.historical_prices.len() - DAY_LENGTH];
                                let yesterday_closing_price =
                                    stonk.historical_prices[stonk.historical_prices.len() - 1];

                                if yesterday_opening_price >= yesterday_closing_price
                                    || yesterday_opening_price == 0
                                {
                                    panic!("This should have been checked before")
                                }

                                let yesterday_gain =
                                    (yesterday_closing_price - yesterday_opening_price) as f64
                                        / yesterday_opening_price as f64;

                                let dividend = (agent.owned_stonks()[stonk_id] as f64
                                    * stonk.current_unit_price_cents() as f64
                                    * DIVIDEND_PAYOUT
                                    * yesterday_gain)
                                    as u32;

                                agent.add_cash(dividend)?;
                            }
                        }
                    }
                }
                GamePhase::Night { .. } => {
                    // At the beginning of the night, set the available events.
                    // We set them here because we need the market data.
                    if agent.render_counter() == 0 && agent.available_night_events().len() == 0 {
                        let mut events = NightEvent::iter()
                            .filter(|e| {
                                match e {
                                    NightEvent::CharacterAssassination { .. }
                                    | NightEvent::ReceiveDividends { .. } => return false,
                                    _ => {}
                                };
                                e.unlock_condition()(agent, &self.stonks)
                            })
                            .collect::<Vec<NightEvent>>();

                        for event in character_assassination_events.iter() {
                            if event.unlock_condition()(agent, &self.stonks) == true {
                                events.push(event.clone());
                            }
                        }

                        // Add ReceiveDividends events for each stonk
                        for stonk in self.stonks.iter() {
                            let event = NightEvent::ReceiveDividends { stonk_id: stonk.id };
                            if event.unlock_condition()(agent, &self.stonks) == true {
                                events.push(event.clone());
                            }
                        }

                        info!("Got events {:#?}", events);
                        events.shuffle(&mut rand::thread_rng());
                        events = events
                            .iter()
                            .take(MAX_EVENTS_PER_NIGHT)
                            .map(|e| e.clone())
                            .collect::<Vec<NightEvent>>();

                        agent.clear_action();
                        agent.clear_event_card();
                        agent.set_available_night_events(events);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Market, HISTORICAL_SIZE};
    use crate::{
        game::agent::{AgentCondition, DecisionAgent, UserAgent},
        ui::ui::{render_stonk, UiOptions, ZoomLevel},
        utils::AppResult,
    };
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::{thread, time::Duration};

    #[test]
    fn test_market() -> AppResult<()> {
        let mut market = Market::new();

        let price_per_share_in_cents = 50 * 100;
        let number_of_shares = 10_000;
        let drift = 0.0;
        let drift_volatility = 0.0002;
        let shock_probability = 0.05;
        let volatility = 0.00005;

        for stonk in market.stonks.iter_mut() {
            stonk.set_test_values(
                price_per_share_in_cents,
                number_of_shares,
                drift,
                drift_volatility,
                volatility,
                shock_probability,
            );
        }

        let rng = &mut ChaCha8Rng::from_entropy();
        while market.last_tick < HISTORICAL_SIZE {
            market.tick_day(rng)
        }

        let mut agent = UserAgent::new("test".into(), [0; 32]);
        agent.add_condition(AgentCondition::UltraVision, market.last_tick + 1);
        let mut ui_options = UiOptions::new();
        ui_options.focus_on_stonk = Some(0);
        ui_options.zoom_level = ZoomLevel::Max;

        // create crossterm terminal to stdout
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend).unwrap();

        let mut idx = 0;
        loop {
            thread::sleep(Duration::from_millis(1000));
            ui_options.focus_on_stonk = Some(idx % market.stonks.len());
            terminal.draw(|frame| {
                let area = frame.area();
                render_stonk(frame, &market, &agent, &ui_options, area)
                    .expect("Failed to render stonk");
            })?;

            if idx == market.stonks.len() * (ZoomLevel::Max as usize + 1) {
                break;
            }
            idx += 1;
        }
        Ok(())
    }
}

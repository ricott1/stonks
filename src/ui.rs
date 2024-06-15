use crate::agent::{DecisionAgent, UserAgent};
use crate::stonk::{GamePhase, Market};
use crate::utils::{img_to_lines, AppResult};
use ratatui::layout::Constraint;
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::text::Line;
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType, Paragraph};
use ratatui::{layout::Layout, Frame};

const STONKS: [&'static str; 6] = [
    "███████╗████████╗ ██████╗ ███╗   ██╗██╗  ██╗███████╗██╗",
    "██╔════╝╚══██╔══╝██╔═══██╗████╗  ██║██║ ██╔╝██╔════╝██║",
    "███████╗   ██║   ██║   ██║██╔██╗ ██║█████╔╝ ███████╗██║",
    "╚════██║   ██║   ██║   ██║██║╚██╗██║██╔═██╗ ╚════██║╚═╝",
    "███████║   ██║   ╚██████╔╝██║ ╚████║██║  ██╗███████║██╗",
    "╚══════╝   ╚═╝    ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝╚══════╝╚═╝",
];

#[derive(Debug, Clone, Copy)]
pub enum UiDisplay {
    Stonks,
    Portfolio,
}
#[derive(Debug, Clone, Copy)]
pub struct UiOptions {
    pub min_y_bound: u16,
    pub max_y_bound: u16,
    pub focus_on_stonk: Option<usize>,
    pub display: UiDisplay,
}

impl UiOptions {
    pub fn new() -> Self {
        UiOptions {
            min_y_bound: 40,
            max_y_bound: 140,
            focus_on_stonk: None,
            display: UiDisplay::Stonks,
        }
    }
}

fn x_ticks(market: &Market) -> Vec<f64> {
    let min_tick = if market.last_tick > market.historical_size as u64 {
        market.last_tick - market.historical_size as u64
    } else {
        0
    };

    (min_tick..market.last_tick).map(|t| t as f64).collect()
}

pub struct Ui<'a> {
    stonk: Vec<Line<'a>>,
}

impl<'a> Ui<'a> {
    fn render_day(
        &mut self,
        frame: &mut Frame,
        market: &Market,
        ui_options: UiOptions,
        agent: &UserAgent,
    ) -> AppResult<()> {
        let area = frame.size();

        let split = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

        let styles = vec![
            Style::default().cyan(),
            Style::default().magenta(),
            Style::default().green(),
            Style::default().red(),
            Style::default().yellow(),
            Style::default().blue(),
        ];

        let mut x_ticks = x_ticks(market);

        let data_size = area.width as usize - 5;
        // We want to take only the last 'data_size' data
        let to_skip = if x_ticks.len() > data_size {
            x_ticks.len() - data_size
        } else {
            0
        };
        x_ticks = x_ticks
            .iter()
            .skip(to_skip)
            .map(|t| *t)
            .collect::<Vec<f64>>();

        let datas = market
            .stonks
            .iter()
            .map(|(_, stonk)| stonk.data(x_ticks.clone()))
            .collect::<Vec<Vec<(f64, f64)>>>();

        let datasets = market
            .stonks
            .iter()
            .enumerate()
            .filter(|(idx, _)| {
                if let Some(stonk_id) = ui_options.focus_on_stonk {
                    *idx == stonk_id
                } else {
                    true
                }
            })
            .map(|(idx, (_, stonk))| {
                Dataset::default()
                    .graph_type(GraphType::Line)
                    .name(format!("{}: {}", idx + 1, stonk.name.clone()))
                    .marker(symbols::Marker::HalfBlock)
                    .style(styles[idx])
                    .data(&datas[idx])
            })
            .collect::<Vec<Dataset>>();

        let mut min_y_bound = ui_options.min_y_bound;
        let mut max_y_bound = ui_options.max_y_bound;
        if let Some(stonk_id) = ui_options.focus_on_stonk {
            if let Some(stonk) = market.stonks.get(&stonk_id) {
                let min_price = stonk
                    .historical_prices
                    .iter()
                    .min()
                    .copied()
                    .unwrap_or_default();
                let max_price = stonk
                    .historical_prices
                    .iter()
                    .max()
                    .copied()
                    .unwrap_or_default();
                if min_price < 2000 {
                    min_y_bound = 0;
                } else {
                    min_y_bound = min_price as u16 / 2000 * 20 - 10;
                }
                if max_price < 2000 {
                    max_y_bound = 40;
                } else {
                    max_y_bound = max_price as u16 / 2000 * 20 + 10;
                }
            }
        }

        let avg_bound = (min_y_bound + max_y_bound) / 2;

        let chart = Chart::new(datasets)
            .block(
                Block::bordered()
                    .title(format!(" Stonk Market: {:?} ", market.phase).cyan().bold()),
            )
            .x_axis(
                Axis::default()
                    .title("Tick")
                    .style(Style::default().gray())
                    .bounds([x_ticks[0], x_ticks[x_ticks.len() - 1]]),
            )
            .y_axis(
                Axis::default()
                    .title(format!("Price"))
                    .style(Style::default().gray())
                    .labels(vec![
                        min_y_bound.to_string().bold(),
                        avg_bound.to_string().bold(),
                        max_y_bound.to_string().bold(),
                    ])
                    .bounds([min_y_bound as f64, max_y_bound as f64]),
            );

        frame.render_widget(chart, split[0]);

        frame.render_widget(
            Paragraph::new(format!("'#':select stonk number '#', enter:reset",)), //, p:portfolio, l:stonks"
            split[1],
        );

        if let Some(stonk_id) = ui_options.focus_on_stonk {
            if let Some(stonk) = market.stonks.get(&stonk_id) {
                frame.render_widget(
                    Paragraph::new(format!(
                        "b: buy for ${:.2}  s: sell for ${:.2}",
                        stonk.formatted_buy_price(),
                        stonk.formatted_sell_price()
                    )),
                    split[2],
                );
                let amount = agent
                    .owned_stonks()
                    .get(&stonk.id)
                    .copied()
                    .unwrap_or_default();
                frame.render_widget(
                    Paragraph::new(format!(
                        "Cash: ${:.2} - {} ({:.03}%)",
                        agent.formatted_cash(),
                        amount,
                        (amount as f64 / stonk.number_of_shares as f64)
                    )),
                    split[3],
                );
            } else {
                frame.render_widget(
                    Paragraph::new(format!("Cash: ${:.2}", agent.formatted_cash(),)),
                    split[3],
                );
            }
        } else {
            frame.render_widget(
                Paragraph::new(format!("Cash: ${:.2}", agent.formatted_cash(),)),
                split[3],
            );
        }

        Ok(())
    }

    pub fn new() -> Self {
        Self {
            stonk: img_to_lines("stonk.png").expect("Cannot load stonk image"),
        }
    }

    fn clear(&self, frame: &mut Frame) {
        let area = frame.size();
        let mut lines = vec![];
        for _ in 0..area.height {
            lines.push(Line::from(" ".repeat(area.width.into())));
        }
        let clear = Paragraph::new(lines).style(Color::White);
        frame.render_widget(clear, area);
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        market: &Market,
        ui_options: UiOptions,
        agent: &UserAgent,
        number_of_players: usize,
    ) -> AppResult<()> {
        self.clear(frame);

        match ui_options.display {
            UiDisplay::Portfolio => {}
            UiDisplay::Stonks => match market.phase {
                GamePhase::Day { .. } => self.render_day(frame, market, ui_options, agent)?,
                GamePhase::Night { counter } => {
                    let area = frame.size();
                    let img_width = 2 * self.stonk.len() as u16;
                    let side_length = if area.width > img_width {
                        (area.width - img_width) / 2
                    } else {
                        0
                    };
                    let split = Layout::horizontal([
                        Constraint::Length(side_length),
                        Constraint::Length(img_width),
                        Constraint::Length(side_length),
                    ])
                    .split(area);

                    let v_split = Layout::vertical([
                        Constraint::Max(img_width / 2),
                        Constraint::Length(2),
                        Constraint::Length(7),
                        Constraint::Length(2),
                    ])
                    .split(split[1]);

                    frame.render_widget(Paragraph::new(self.stonk.clone()), v_split[0]);
                    frame.render_widget(
                        Paragraph::new(format!(
                            "{} player{} online!\nGet ready to",
                            number_of_players,
                            if number_of_players > 1 { "s" } else { "" }
                        ))
                        .centered(),
                        v_split[1],
                    );
                    frame.render_widget(
                        Paragraph::new(
                            STONKS
                                .iter()
                                .map(|&s| Line::from(s).style(Style::default().green()))
                                .collect::<Vec<Line>>(),
                        )
                        .centered(),
                        v_split[2],
                    );
                    frame.render_widget(
                        Paragraph::new(format!("in {}", counter)).centered(),
                        v_split[3],
                    );
                }
            },
        }

        Ok(())
    }
}

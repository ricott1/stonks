use crate::agent::{DecisionAgent, UserAgent};
use crate::stonk::{GamePhase, Market};
use crate::utils::{img_to_lines, AppResult};
use ratatui::layout::{Constraint, Margin};
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

        let split = Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).split(area);

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
            .map(|(id, stonk)| {
                if let Some(stonk_id) = ui_options.focus_on_stonk {
                    if stonk_id == *id {
                        stonk.data(x_ticks.clone())
                    } else {
                        vec![]
                    }
                } else {
                    stonk.data(x_ticks.clone())
                }
            })
            .collect::<Vec<Vec<(f64, f64)>>>();

        let datasets = market
            .stonks
            .iter()
            .enumerate()
            .map(|(idx, (_, stonk))| {
                Dataset::default()
                    .graph_type(GraphType::Line)
                    .name(stonk.name.clone())
                    .marker(symbols::Marker::HalfBlock)
                    .style(styles[idx])
                    .data(&datas[idx])
            })
            .collect::<Vec<Dataset>>();

        let avg_bound = (ui_options.min_y_bound + ui_options.max_y_bound) / 2;

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
                        ui_options.min_y_bound.to_string().bold(),
                        avg_bound.to_string().bold(),
                        ui_options.max_y_bound.to_string().bold(),
                    ])
                    .bounds([ui_options.min_y_bound as f64, ui_options.max_y_bound as f64]),
            );

        frame.render_widget(chart, split[0]);

        let f_split = Layout::horizontal([40, 40]).split(split[1]);
        let porfolio = if let Some(stonk_id) = ui_options.focus_on_stonk {
            let stonk_amount = if let Some(stonk) = market.stonks.get(&stonk_id) {
                let amount = agent
                    .owned_stonks()
                    .get(&stonk_id)
                    .copied()
                    .unwrap_or_default();
                format!("{} {}", stonk.name, amount)
            } else {
                "".to_string()
            };
            format!("Cash: ${:.0} - {}", agent.cash(), stonk_amount)
        } else {
            format!("Cash: ${:.0}", agent.cash())
        };

        frame.render_widget(Paragraph::new(porfolio), f_split[0]);
        frame.render_widget(
            Paragraph::new("#:select, enter:reset, b:buy, s:sell, p:portfolio, l:stonks"),
            f_split[1],
        );

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
    ) -> AppResult<()> {
        self.clear(frame);

        match ui_options.display {
            UiDisplay::Portfolio => {}
            UiDisplay::Stonks => match market.phase {
                GamePhase::Day { .. } => self.render_day(frame, market, ui_options, agent)?,
                GamePhase::Night { .. } => {
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

                    let v_split =
                        Layout::vertical([Constraint::Max(img_width / 2), Constraint::Length(8)])
                            .split(split[1]);

                    frame.render_widget(Paragraph::new(self.stonk.clone()), v_split[0]);
                    frame.render_widget(
                        Paragraph::new(
                            STONKS
                                .iter()
                                .map(|&s| Line::from(s).style(Style::default().green()))
                                .collect::<Vec<Line>>(),
                        )
                        .centered(),
                        v_split[1].inner(&Margin {
                            vertical: 1,
                            horizontal: 1,
                        }),
                    );
                }
            },
        }

        Ok(())
    }
}

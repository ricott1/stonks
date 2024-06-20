use crate::agent::{DecisionAgent, UserAgent};
use crate::market::{GamePhase, Market, DAY_LENGTH, HISTORICAL_SIZE, NIGHT_LENGTH};
use crate::stonk::Stonk;
use crate::utils::{img_to_lines, AppResult};
use crossterm::event::KeyCode;
use once_cell::sync::Lazy;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::style::palette::tailwind;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Axis, Block, Borders, Cell, Chart, Dataset, GraphType, HighlightSpacing, Paragraph, Row, Table,
    TableState,
};
use ratatui::{layout::Layout, Frame};
use std::fmt::{self};

const STONKS: [&'static str; 6] = [
    "███████╗████████╗ ██████╗ ███╗   ██╗██╗  ██╗███████╗██╗",
    "██╔════╝╚══██╔══╝██╔═══██╗████╗  ██║██║ ██╔╝██╔════╝██║",
    "███████╗   ██║   ██║   ██║██╔██╗ ██║█████╔╝ ███████╗██║",
    "╚════██║   ██║   ██║   ██║██║╚██╗██║██╔═██╗ ╚════██║╚═╝",
    "███████║   ██║   ╚██████╔╝██║ ╚████║██║  ██╗███████║██╗",
    "╚══════╝   ╚═╝    ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝╚══════╝╚═╝",
];

static STONKS_LINES: Lazy<Vec<Line>> = Lazy::new(|| {
    STONKS
        .iter()
        .map(|&s| Line::from(s).style(Style::default().green()))
        .collect::<Vec<Line>>()
});

static STONKS_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| {
    (1..=13)
        .map(|n| {
            img_to_lines(format!("card{:02}.png", n).as_str()).expect("Cannot load stonk image")
        })
        .collect::<Vec<Vec<Line>>>()
});

static UNSELECTED_CARD: Lazy<Vec<Line>> =
    Lazy::new(|| img_to_lines("unselected_card.png").expect("Cannot load stonk image"));

const CARD_WIDTH: u16 = 28;
const CARD_HEIGHT: u16 = 40 / 2;

const PALETTES: [tailwind::Palette; 5] = [
    tailwind::BLUE,
    tailwind::EMERALD,
    tailwind::INDIGO,
    tailwind::RED,
    tailwind::LIME,
];

struct TableColors {
    buffer_bg: Color,
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_style_fg: Color,
    normal_row_color: Color,
    alt_row_color: Color,
}

impl TableColors {
    const fn new(color: &tailwind::Palette) -> Self {
        Self {
            buffer_bg: tailwind::SLATE.c950,
            header_bg: color.c900,
            header_fg: tailwind::SLATE.c200,
            row_fg: tailwind::SLATE.c200,
            selected_style_fg: color.c400,
            normal_row_color: tailwind::SLATE.c950,
            alt_row_color: tailwind::SLATE.c800,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UiDisplay {
    Stonks,
    Portfolio,
}

#[derive(Debug, Clone, Copy)]
pub enum ZoomLevel {
    Short,
    Medium,
    Long,
    Max,
}

impl fmt::Display for ZoomLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ZoomLevel::Short => write!(f, "Short"),
            ZoomLevel::Medium => write!(f, "Medium"),
            ZoomLevel::Long => write!(f, "Long"),
            ZoomLevel::Max => write!(f, "Max"),
        }
    }
}

impl ZoomLevel {
    pub fn next(&self) -> Self {
        match self {
            Self::Short => Self::Medium,
            Self::Medium => Self::Long,
            Self::Long => Self::Max,
            Self::Max => Self::Short,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UiOptions {
    pub user_id: String,
    pub focus_on_stonk: Option<usize>,
    display: UiDisplay,
    selected_stonk_index: usize,
    palette_index: usize,
    zoom_level: ZoomLevel,
    pub render_counter: usize,
    pub selected_event_card: Option<usize>,
}

impl UiOptions {
    pub fn new(user_id: String) -> Self {
        UiOptions {
            user_id,
            focus_on_stonk: None,
            display: UiDisplay::Stonks,
            selected_stonk_index: 0,
            palette_index: 0,
            zoom_level: ZoomLevel::Short,
            render_counter: 0,
            selected_event_card: None,
        }
    }

    pub fn handle_key_events(&mut self, key_code: KeyCode, agent: &UserAgent) -> AppResult<()> {
        let num_night_events = agent.available_night_events().len();
        match key_code {
            crossterm::event::KeyCode::Down => {
                if let Some(index) = self.focus_on_stonk {
                    self.focus_on_stonk = Some((index + 1) % 8)
                } else {
                    self.selected_stonk_index = (self.selected_stonk_index + 1) % 8;
                }
            }

            crossterm::event::KeyCode::Up => {
                if let Some(index) = self.focus_on_stonk {
                    self.focus_on_stonk = Some((index + 8 - 1) % 8)
                } else {
                    self.selected_stonk_index = (self.selected_stonk_index + 8 - 1) % 8;
                }
            }

            crossterm::event::KeyCode::Left => {
                if agent.selected_action().is_none() && num_night_events > 0 {
                    if let Some(idx) = self.selected_event_card {
                        self.selected_event_card =
                            Some((idx + num_night_events - 1) % num_night_events)
                    } else {
                        self.selected_event_card = Some(0)
                    }
                }
            }

            crossterm::event::KeyCode::Right => {
                if agent.selected_action().is_none() && num_night_events > 0 {
                    if let Some(idx) = self.selected_event_card {
                        self.selected_event_card = Some((idx + 1) % num_night_events)
                    } else {
                        self.selected_event_card = Some(0)
                    }
                }
            }

            crossterm::event::KeyCode::Char('z') => self.zoom_level = self.zoom_level.next(),

            crossterm::event::KeyCode::Char('c') => {
                self.palette_index = (self.palette_index + 1) % PALETTES.len();
            }
            crossterm::event::KeyCode::Char('p') => self.display = UiDisplay::Portfolio,
            crossterm::event::KeyCode::Char('l') => self.display = UiDisplay::Stonks,

            _ => {
                for idx in 1..9 {
                    if key_code
                        == crossterm::event::KeyCode::Char(
                            format!("{idx}").chars().next().unwrap_or_default(),
                        )
                    {
                        self.reset();
                        self.focus_on_stonk = Some(idx - 1);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        self.focus_on_stonk = None;
        self.zoom_level = ZoomLevel::Short;
        self.selected_stonk_index = 0;
    }

    pub fn select_stonk(&mut self) {
        let idx = self.selected_stonk_index;
        self.reset();
        self.focus_on_stonk = Some(idx);
    }
}

fn build_stonks_table<'a>(market: &Market, agent: &UserAgent, colors: TableColors) -> Table<'a> {
    let header_style = Style::default().fg(colors.header_fg).bg(colors.header_bg);
    let selected_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .fg(colors.selected_style_fg);

    let header = [
        "Stonk",
        "Buy $",
        "Sell $",
        "Today +/-",
        "Max +/-",
        "Owned",
        "Value",
    ]
    .into_iter()
    .map(Cell::from)
    .collect::<Row>()
    .style(header_style)
    .height(1);

    let rows = market.stonks.iter().enumerate().map(|(i, stonk)| {
        let color = match i % 2 {
            0 => colors.normal_row_color,
            _ => colors.alt_row_color,
        };

        let n = market.last_tick % DAY_LENGTH;
        let style = if n > 0 {
            let last_n_prices = stonk.historical_prices.iter().rev().take(n);
            let last_len = last_n_prices.len() as f64;
            let mut last_minute_avg_price = 0.0;

            for p in last_n_prices {
                last_minute_avg_price += *p as f64 / last_len;
            }

            if last_minute_avg_price > stonk.price_per_share_in_cents as f64 / 4.0 * 5.0 {
                Style::default().red()
            } else if last_minute_avg_price > stonk.price_per_share_in_cents as f64 {
                Style::default().yellow()
            } else if last_minute_avg_price < stonk.price_per_share_in_cents as f64 / 4.0 * 5.0 {
                Style::default().light_green()
            } else if last_minute_avg_price < stonk.price_per_share_in_cents as f64 {
                Style::default().green()
            } else {
                Style::default()
            }
        } else {
            Style::default()
        };

        let today_initial_price = stonk.historical_prices[stonk.historical_prices.len() - n - 1];
        let today_variation = if today_initial_price > 0 {
            (stonk.price_per_share_in_cents as f64 - today_initial_price as f64)
                / today_initial_price as f64
                * 100.0
        } else {
            0.0
        };

        let today_style = if today_variation > 5.0 {
            Style::default().green()
        } else if today_variation >= 1.0 {
            Style::default().green()
        } else if today_variation >= 0.1 {
            Style::default().light_green()
        } else if today_variation <= -1.0 {
            Style::default().red()
        } else if today_variation <= -0.1 {
            Style::default().yellow()
        } else {
            Style::default()
        };

        let max_variation = if stonk.historical_prices[0] > 0 {
            (stonk.price_per_share_in_cents as f64 - stonk.historical_prices[0] as f64)
                / stonk.historical_prices[0] as f64
                * 100.0
        } else {
            0.0
        };

        let max_style = if max_variation > 5.0 {
            Style::default().green()
        } else if max_variation >= 5.0 {
            Style::default().green()
        } else if max_variation >= 0.5 {
            Style::default().light_green()
        } else if max_variation <= -5.0 {
            Style::default().red()
        } else if max_variation <= -0.5 {
            Style::default().yellow()
        } else {
            Style::default()
        };

        let agent_share = agent.owned_stonks()[stonk.id] as f64 / stonk.number_of_shares as f64;

        let agent_style = if agent_share >= 5.0 {
            Style::default().magenta()
        } else if agent_share >= 1.0 {
            Style::default().light_magenta()
        } else if agent_share >= 0.1 {
            Style::default().cyan()
        } else if agent_share >= 0.01 {
            Style::default().light_cyan()
        } else {
            Style::default()
        };

        let agent_stonk_value = (agent.owned_stonks()[stonk.id] as f64
            * (stonk.price_per_share_in_cents as f64 / 100.0))
            as u32;

        let agent_stonk_style = if agent_stonk_value > 0 {
            style
        } else {
            Style::default()
        };

        Row::new(vec![
            Cell::new(format!("\n{}", stonk.name)),
            Cell::new(format!("\n${:.2}", stonk.formatted_buy_price())).style(style),
            Cell::new(format!("\n${:.2}", stonk.formatted_sell_price())).style(style),
            Cell::new(format!("\n{:+.2}%", today_variation)).style(today_style),
            Cell::new(format!("\n{:+.2}%", max_variation)).style(max_style),
            Cell::new(format!("\n{:.2}%", agent_share)).style(agent_style),
            Cell::new(format!("\n${}", agent_stonk_value)).style(agent_stonk_style),
        ])
        .style(Style::new().fg(colors.row_fg).bg(color))
        .height(3)
    });
    let bar = " █ ";
    Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .highlight_style(selected_style)
    .highlight_symbol(Text::from(vec![bar.into(), bar.into(), bar.into()]))
    .bg(colors.buffer_bg)
    .highlight_spacing(HighlightSpacing::Always)
}

fn render_day(
    frame: &mut Frame,
    market: &Market,
    agent: &UserAgent,
    ui_options: &UiOptions,
    area: Rect,
) -> AppResult<()> {
    if let Some(stonk_id) = ui_options.focus_on_stonk {
        let stonk = &market.stonks[stonk_id];
        render_stonk(frame, market, ui_options, stonk, area)?;
    } else {
        let colors = TableColors::new(&PALETTES[ui_options.palette_index]);
        let table = build_stonks_table(market, agent, colors);
        frame.render_stateful_widget(
            table,
            area,
            &mut TableState::default().with_selected(Some(ui_options.selected_stonk_index)),
        );
    }
    Ok(())
}

fn render_night(
    frame: &mut Frame,
    counter: usize,
    agent: &UserAgent,
    ui_options: &UiOptions,
    area: Rect,
) -> AppResult<()> {
    let total_width = CARD_WIDTH * 3 + 18;
    let side_length = if area.width > total_width {
        (area.width - total_width) / 2
    } else {
        0
    };
    let split = Layout::horizontal([
        Constraint::Length(side_length),
        Constraint::Length(total_width),
        Constraint::Length(side_length),
    ])
    .split(area);

    let v_split = Layout::vertical([
        Constraint::Length(7),
        Constraint::Length(2),
        Constraint::Max(CARD_HEIGHT + 2),
        Constraint::Length(2),
    ])
    .split(split[1].inner(&Margin {
        horizontal: 0,
        vertical: 1,
    }));

    let num_night_events = agent.available_night_events().len();

    if num_night_events > 0 {
        let cards_split =
            Layout::horizontal([Constraint::Length(CARD_WIDTH + 4)].repeat(num_night_events))
                .split(v_split[2]);

        for i in 0..num_night_events {
            // If there is not more than half of the time still available, skip the animation
            if counter < NIGHT_LENGTH / 2 && ui_options.render_counter < 3 * STONKS_CARDS.len() {
                frame.render_widget(
                    Paragraph::new(
                        STONKS_CARDS[(ui_options.render_counter / 3) % STONKS_CARDS.len()].clone(),
                    ),
                    cards_split[i].inner(&Margin {
                        horizontal: 2,
                        vertical: 1,
                    }),
                );
            } else {
                if ui_options.selected_event_card.is_some()
                    && ui_options.selected_event_card.unwrap() == i
                {
                    let border_style = if agent.selected_action().is_some() {
                        Style::default().green().on_green()
                    } else {
                        Style::default().red().on_red()
                    };
                    frame.render_widget(
                        Paragraph::new(STONKS_CARDS[STONKS_CARDS.len() - 1].clone())
                            .block(Block::bordered().border_style(border_style)),
                        cards_split[i].inner(&Margin {
                            horizontal: 1,
                            vertical: 0,
                        }),
                    );
                    frame.render_widget(
                        Block::bordered()
                            .border_style(border_style)
                            .borders(Borders::RIGHT | Borders::LEFT),
                        cards_split[i],
                    );
                } else {
                    if agent.selected_action().is_some() {
                        frame.render_widget(
                            Paragraph::new(UNSELECTED_CARD.clone()),
                            cards_split[i].inner(&Margin {
                                horizontal: 2,
                                vertical: 1,
                            }),
                        );
                    } else {
                        frame.render_widget(
                            Paragraph::new(STONKS_CARDS[STONKS_CARDS.len() - 1].clone()),
                            cards_split[i].inner(&Margin {
                                horizontal: 2,
                                vertical: 1,
                            }),
                        );
                    }
                }
                frame.render_widget(
                    Paragraph::new(
                        agent.available_night_events()[i]
                            .description()
                            .iter()
                            .map(|l| Line::from(*l).bold().black())
                            .collect::<Vec<Line>>(),
                    )
                    .centered(),
                    cards_split[i].inner(&Margin {
                        horizontal: 3,
                        vertical: 3,
                    }),
                );
            }
        }
    } else {
        frame.render_widget(
            Paragraph::new("No special events available tonight").centered(),
            v_split[2],
        );
    }
    frame.render_widget(Paragraph::new(STONKS_LINES.clone()).centered(), v_split[0]);

    Ok(())
}

fn render_stonk(
    frame: &mut Frame,
    market: &Market,
    ui_options: &UiOptions,
    stonk: &Stonk,
    area: Rect,
) -> AppResult<()> {
    let styles = vec![
        Style::default().cyan(),
        Style::default().magenta(),
        Style::default().green(),
        Style::default().red(),
        Style::default().yellow(),
        Style::default().blue(),
        Style::default().white(),
        Style::default().light_magenta(),
    ];

    let graph_width = area.width as usize - 5;

    let clustering = match ui_options.zoom_level {
        ZoomLevel::Short => 1,
        ZoomLevel::Medium => 4,
        ZoomLevel::Long => 16,
        ZoomLevel::Max => HISTORICAL_SIZE / graph_width,
    };

    let x_data: Vec<f64> = (0..market.last_tick)
        .rev()
        .take((clustering * graph_width).min(stonk.historical_prices.len()))
        .rev()
        .map(|t| t as f64)
        .collect();

    let y_data: Vec<f64> = stonk
        .historical_prices
        .iter()
        .rev()
        .take((clustering * graph_width).min(stonk.historical_prices.len()))
        .rev()
        .map(|v| *v as f64 / 100.0)
        .collect();

    assert!(x_data.len() == y_data.len());

    let datas: Vec<(f64, f64)> = x_data
        .iter()
        .enumerate()
        .step_by(clustering)
        .map(|(idx, _)| {
            let max_idx = (idx + clustering).min(x_data.len());
            (
                x_data[idx],
                y_data[idx..max_idx].iter().sum::<f64>() / (max_idx - idx) as f64,
            )
        })
        .collect();

    let datasets = vec![Dataset::default()
        .graph_type(GraphType::Line)
        .marker(symbols::Marker::HalfBlock)
        .style(styles[stonk.id])
        .data(&datas)];

    let min_y_bound;
    let max_y_bound;

    let min_price = datas
        .iter()
        .map(|(_, d)| *d as usize)
        .min()
        .unwrap_or_default();
    let max_price = datas
        .iter()
        .map(|(_, d)| *d as usize)
        .max()
        .unwrap_or_default();

    if min_price < 20 {
        min_y_bound = 0;
    } else {
        min_y_bound = min_price / 20 * 20 - 20;
    }
    if max_price < 20 {
        max_y_bound = 40;
    } else {
        max_y_bound = max_price / 20 * 20 + 20;
    }

    let n_y_labels = area.height as usize / 6;
    let y_labels: Vec<Span<'static>> = (0..n_y_labels)
        .map(|r| {
            format!(
                "{:>6}",
                (min_y_bound + r * (max_y_bound - min_y_bound) / n_y_labels)
            )
            .bold()
        })
        .collect();

    let min_x_bound = x_data[0] as usize;
    let max_x_bound = x_data[x_data.len() - 1] as usize;
    let x_labels = [min_x_bound, max_x_bound]
        .iter()
        .map(|v| v.to_string().bold())
        .collect();

    let chart = Chart::new(datasets)
        .block(
            Block::bordered()
                .title(format!(" Stonk Market: {} ", stonk.name))
                .style(styles[stonk.id])
                .bold(),
        )
        .x_axis(
            Axis::default()
                .title(format!("Tick (x{})", clustering))
                .labels_alignment(ratatui::layout::Alignment::Center)
                .style(Style::default().gray())
                .labels(x_labels)
                .bounds([min_x_bound as f64, max_x_bound as f64]),
        )
        .y_axis(
            Axis::default()
                .title(format!(
                    "Price ${}",
                    (stonk.price_per_share_in_cents as f64 / 100.0) as u32
                ))
                .style(Style::default().gray())
                .labels(y_labels)
                .bounds([min_y_bound as f64, max_y_bound as f64]),
        );

    frame.render_widget(chart, area);

    Ok(())
}

fn clear(frame: &mut Frame) {
    let area = frame.size();
    let mut lines = vec![];
    for _ in 0..area.height {
        lines.push(Line::from(" ".repeat(area.width.into())));
    }
    let clear = Paragraph::new(lines).style(Color::White);
    frame.render_widget(clear, area);
}

fn render_header(
    frame: &mut Frame,
    market: &Market,
    agent: &UserAgent,
    ui_options: &UiOptions,
    number_of_players: usize,
    area: Rect,
) {
    let extra_text = if let Some(stonk_id) = ui_options.focus_on_stonk {
        let amount = agent.owned_stonks()[stonk_id];
        let stonk = &market.stonks[stonk_id];
        format!(
            "Owned shares {} ({:.03}%) ",
            amount,
            (amount as f64 / stonk.number_of_shares as f64)
        )
    } else {
        format!(
            "{} player{} online - `ssh {}@frittura.org -p 3333`",
            number_of_players,
            if number_of_players > 1 { "s" } else { "" },
            ui_options.user_id
        )
    };

    frame.render_widget(
        Paragraph::new(format!(
            "Day {:<5} {} - Cash: ${:<6.2} - {}",
            market.cycles + 1,
            market.phase.formatted_time(),
            agent.formatted_cash(),
            extra_text,
        )),
        area,
    );
}

fn render_footer(
    frame: &mut Frame,
    market: &Market,
    agent: &UserAgent,
    ui_options: &UiOptions,
    area: Rect,
) {
    let mut lines: Vec<Line> = vec![];

    match market.phase {
        GamePhase::Day { .. } => {
            if let Some(stonk_id) = ui_options.focus_on_stonk {
                lines.push(
                    format!(
                        "{:28} {:28} {:28}",
                        "`↑↓`:select stonk", "`return`:main table", "`z`:zoom level",
                    )
                    .into(),
                );

                let stonk = &market.stonks[stonk_id];

                let max_buy_amount = agent.cash() / stonk.buy_price();
                lines.push(
                    format!(
                        "{:28} {:28} {:28}",
                        format!("`b`: buy  x1 (${:.2})", stonk.formatted_buy_price()),
                        format!(
                            "`B`: buy  x100 (${:.2})",
                            100.0 * stonk.formatted_buy_price()
                        ),
                        format!(
                            "`m`: buy  x{} (${:.2})",
                            max_buy_amount,
                            max_buy_amount as f64 * stonk.formatted_buy_price()
                        ),
                    )
                    .into(),
                );
                let owned_amount = agent.owned_stonks()[stonk.id];
                lines.push(
                    format!(
                        "{:28} {:28} {:28}",
                        format!("`s`: sell x1 (${:.2})", stonk.formatted_sell_price()),
                        format!(
                            "`S`: sell x100 (${:.2})",
                            100.0 * stonk.formatted_sell_price()
                        ),
                        format!(
                            "`d`: sell x{} (${:.2})",
                            owned_amount,
                            owned_amount as f64 * stonk.formatted_sell_price()
                        ),
                    )
                    .into(),
                );
            } else {
                lines.push(
                    format!(
                        "{:28} {:28}",
                        "`↑↓`:select stonk", "`return`:focus on stonk",
                    )
                    .into(),
                );
            }
        }
        GamePhase::Night { .. } => {
            if ui_options.selected_event_card.is_some() && agent.selected_action().is_some() {
                let event = agent.available_night_events()[ui_options.selected_event_card.unwrap()];
                lines.push(format!("You selected `{}`", event).into());
            } else {
                lines.push(format!("{:28} {:28}", "`←→`:select event", "`return`:confirm",).into());
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

pub fn render(
    frame: &mut Frame,
    market: &Market,
    agent: &UserAgent,
    ui_options: &UiOptions,
    number_of_players: usize,
) -> AppResult<()> {
    clear(frame);

    let area = frame.size();
    let split = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(area);

    self::render_header(
        frame,
        market,
        agent,
        ui_options,
        number_of_players,
        split[0],
    );

    match ui_options.display {
        UiDisplay::Portfolio => {}
        UiDisplay::Stonks => match market.phase {
            GamePhase::Day { .. } => render_day(frame, market, agent, ui_options, split[1])?,
            GamePhase::Night { counter } => {
                render_night(frame, counter, agent, ui_options, split[1])?
            }
        },
    }

    self::render_footer(frame, market, agent, ui_options, split[2]);

    Ok(())
}

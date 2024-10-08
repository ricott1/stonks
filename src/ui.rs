use crate::agent::{AgentCondition, DecisionAgent, UserAgent};
use crate::events::{EventRarity, NightEvent};
use crate::market::{
    GamePhase, Market, DAY_LENGTH, HISTORICAL_SIZE, MAX_EVENTS_PER_NIGHT, NIGHT_LENGTH,
};
use crate::stonk::DollarValue;
use crate::utils::*;
use crossterm::event::KeyCode;
use once_cell::sync::Lazy;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::style::palette::tailwind;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Axis, Block, Borders, Cell, Chart, Dataset, GraphType, HighlightSpacing, Paragraph, Row, Table,
    TableState, Wrap,
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

const NO_ANIMATION_FRAMES: usize = 6;
const CARD_ANIMATION_FRAMES: usize = NO_ANIMATION_FRAMES + CARD_WIDTH as usize + 1;
const ANIMATION_RATE: usize = 2;

fn image_to_cards(path: &str) -> Vec<Vec<Line>> {
    let back_image = read_image(path).expect("Cannot load image from file");
    let back_lines = img_to_lines(&back_image).expect("Cannot convert image to lines");
    let front_image = read_image("images/card_front.png").expect("Cannot load image from file");

    // The whole night lasts for NIGHT_LENGTH (6 * 4 = 24 at the moment) seconds.
    // The game renders at 20 FPS, so we got 480 frames total. We want to finish the card animation
    // in 40 frames (2 seconds).

    (0..CARD_ANIMATION_FRAMES)
        .map(|n| {
            let mut idx = n;
            // Initial static card back
            if idx < NO_ANIMATION_FRAMES {
                return back_lines.clone();
            }

            idx -= NO_ANIMATION_FRAMES;
            // card back shrinking animation
            if idx < CARD_WIDTH as usize / 2 {
                let nwidth = CARD_WIDTH as u32 - 2 * idx as u32;
                let nheight = CARD_HEIGHT as u32;
                let resized_image =
                    resize_image(&back_image, nwidth, nheight).expect("Cannot resize image");
                let lines = img_to_lines(&resized_image).expect("Cannot convert image to lines");

                return lines
                    .iter()
                    .map(|l| {
                        let mut spans = vec![];

                        spans.push(Span::raw(
                            " ".repeat((CARD_WIDTH as usize - nwidth as usize) / 2),
                        ));
                        for l in l.spans.iter() {
                            spans.push(l.clone());
                        }
                        spans.push(Span::raw(
                            " ".repeat((CARD_WIDTH as usize - nwidth as usize) / 2),
                        ));

                        Line::from(spans)
                    })
                    .collect();
            }

            idx -= CARD_WIDTH as usize / 2;
            // card front expanding animation
            if idx < CARD_WIDTH as usize / 2 {
                let nwidth = 2 * idx as u32;
                let nheight = CARD_HEIGHT as u32;
                let resized_image =
                    resize_image(&front_image, nwidth, nheight).expect("Cannot resize image");
                let lines = img_to_lines(&resized_image).expect("Cannot convert image to lines");

                return lines
                    .iter()
                    .map(|l| {
                        let mut spans = vec![];

                        spans.push(Span::raw(
                            " ".repeat((CARD_WIDTH as usize - nwidth as usize) / 2),
                        ));
                        for l in l.spans.iter() {
                            spans.push(l.clone());
                        }
                        spans.push(Span::raw(
                            " ".repeat((CARD_WIDTH as usize - nwidth as usize) / 2),
                        ));

                        Line::from(spans)
                    })
                    .collect();
            }

            //card front
            img_to_lines(&front_image).expect("Cannot convert image to lines")
        })
        .collect::<Vec<Vec<Line>>>()
}

static STONKS_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| image_to_cards("images/stonks.png"));
static DOGE_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| image_to_cards("images/doge.png"));
static ELON_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| image_to_cards("images/elon.png"));
static KIM_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| image_to_cards("images/kim.png"));

static UNSELECTED_CARD: Lazy<Vec<Line>> = Lazy::new(|| {
    let image = read_image("images/unselected_card.png").expect("Cannot load image from file");
    img_to_lines(&image).expect("Cannot load stonk image")
});

trait Carded {
    fn cards(&self) -> &Vec<Vec<Line>>;
}

impl Carded for NightEvent {
    fn cards(&self) -> &Vec<Vec<Line>> {
        match self.rarity() {
            EventRarity::Common => &*STONKS_CARDS,
            EventRarity::Uncommon => &*DOGE_CARDS,
            EventRarity::Rare => &*ELON_CARDS,
        }
    }
}

const CARD_WIDTH: u16 = 30;
const CARD_HEIGHT: u16 = 40;

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

#[derive(Debug, Default, Clone, Copy)]
pub enum UiDisplay {
    #[default]
    Stonks,
    Portfolio,
}

#[derive(Debug, Default, Clone, Copy)]
pub enum ZoomLevel {
    #[default]
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

trait Styled {
    fn style(&self) -> Style;
    fn ustyle(&self) -> Style;
}

impl Styled for f64 {
    fn style(&self) -> Style {
        if *self >= 1.0 {
            Style::default().green()
        } else if *self >= 0.1 {
            Style::default().light_green()
        } else if *self <= -1.0 {
            Style::default().red()
        } else if *self <= -0.1 {
            Style::default().yellow()
        } else {
            Style::default()
        }
    }

    fn ustyle(&self) -> Style {
        if *self > 50.0 {
            Style::default().magenta()
        } else if *self >= 10.0 {
            Style::default().cyan()
        } else if *self >= 5.0 {
            Style::default().light_cyan()
        } else if *self >= 1.0 {
            Style::default().green()
        } else if *self >= 0.1 {
            Style::default().light_green()
        } else {
            Style::default()
        }
    }
}

impl Styled for u64 {
    fn style(&self) -> Style {
        (*self as f64).style()
    }
    fn ustyle(&self) -> Style {
        (*self as f64).ustyle()
    }
}

#[derive(Debug, Default, Clone)]
pub struct UiOptions {
    pub focus_on_stonk: Option<usize>,
    display: UiDisplay,
    pub selected_stonk_index: usize,
    palette_index: usize,
    pub(crate) zoom_level: ZoomLevel,
    pub render_counter: usize,
    pub selected_event_card_index: usize,
}

impl UiOptions {
    pub fn new() -> Self {
        UiOptions::default()
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
                    let idx = self.selected_event_card_index;
                    self.selected_event_card_index =
                        (idx + num_night_events - 1) % num_night_events;
                }
            }

            crossterm::event::KeyCode::Right => {
                if agent.selected_action().is_none() && num_night_events > 0 {
                    let idx = self.selected_event_card_index;
                    self.selected_event_card_index = (idx + 1) % num_night_events
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
    let header_style = Style::default()
        .fg(colors.header_fg)
        .bg(colors.header_bg)
        .bold();
    let selected_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .fg(colors.selected_style_fg);

    let header = [
        "Stonk",
        "Buy $",
        "Sell $",
        "Today +/-",
        "Max +/-",
        "Stake",
        "Value",
        "Market cap",
        "Top dogs",
    ]
    .into_iter()
    .map(Cell::from)
    .collect::<Row>()
    .style(header_style)
    .height(1);

    let mut avg_today_variation = 0.0;
    let mut avg_max_variation = 0.0;
    let mut avg_agent_share = 0.0;
    let mut total_agent_stonk_value = 0;

    let mut rows = market
        .stonks
        .iter()
        .filter(|stonk| stonk.historical_prices.len() > 0)
        .enumerate()
        .map(|(i, stonk)| {
            let color = match i % 2 {
                0 => colors.normal_row_color,
                _ => colors.alt_row_color,
            };

            let n = market.last_tick % DAY_LENGTH;
            let today_initial_price = if stonk.historical_prices.len() > n {
                stonk.historical_prices[stonk.historical_prices.len() - n - 1]
            } else {
                0
            };

            let today_variation = if today_initial_price > 0 {
                (stonk.current_unit_price_cents() as f64 - today_initial_price as f64)
                    / (today_initial_price as f64)
                    * 100.0
            } else {
                0.0
            };

            avg_today_variation += today_variation * stonk.number_of_shares as f64;

            let today_style = today_variation.style();

            let max_variation = (stonk.current_unit_price_cents() as f64
                - stonk.starting_price as f64)
                / (stonk.starting_price as f64)
                * 100.0;

            avg_max_variation += max_variation * stonk.number_of_shares as f64;

            let max_style = (max_variation / 10.0).style();

            let agent_share = stonk.to_stake(agent.owned_stonks()[stonk.id]) * 100.0;
            avg_agent_share += agent_share * stonk.number_of_shares as f64;
            let agent_style = agent_share.ustyle();

            let agent_stonk_value =
                agent.owned_stonks()[stonk.id] as u64 * stonk.current_unit_price_cents() as u64;
            total_agent_stonk_value += agent_stonk_value;

            let agent_stonk_style = if agent_stonk_value > 0 {
                today_style
            } else {
                Style::default()
            };

            let top_shareholders = stonk
                .shareholders
                .iter()
                .take(3)
                .map(|(holder, amount)| {
                    let agent_share = stonk.to_stake(*amount) * 100.0;
                    let agent_style = agent_share.ustyle();
                    Line::from(format!("{} {:.03}%", holder, agent_share)).style(agent_style)
                })
                .collect::<Vec<Line>>();

            let market_cap_text = format!("\n${}", stonk.market_cap_cents().format());

            Row::new(vec![
                Cell::new(format!("\n{}", stonk.name)),
                Cell::new(format!("\n${}", stonk.buy_price_cents(1).format()))
                    .style(Style::default()),
                Cell::new(format!("\n${}", stonk.sell_price_cents(1).format()))
                    .style(Style::default()),
                Cell::new(format!("\n{:+.2}%", today_variation)).style(today_style),
                Cell::new(format!("\n{:+.2}%", max_variation)).style(max_style),
                Cell::new(format!("\n{:.03}%", agent_share)).style(agent_style),
                Cell::new(format!("\n${}", agent_stonk_value.format())).style(agent_stonk_style),
                Cell::new(market_cap_text).style(max_style),
                Cell::new(top_shareholders),
            ])
            .style(Style::new().fg(colors.row_fg).bg(color))
            .height(3)
        })
        .collect::<Vec<Row>>();

    let total_number_of_shares = market
        .stonks
        .iter()
        .map(|stonk| stonk.number_of_shares as u64)
        .sum::<u64>() as f64;

    avg_today_variation /= total_number_of_shares;
    avg_max_variation /= total_number_of_shares;
    avg_agent_share /= total_number_of_shares;

    let total_market_cap_text = format!("\n${}", market.total_market_cap().format());

    let total_max_variation_style = (avg_max_variation / 10.0).style();

    let top_portfolios = market
        .portfolios
        .iter()
        .take(3)
        .map(|(holder, amount)| {
            let amount_text = format!("${}", amount.format());

            Line::from(format!("{} {}", holder, amount_text))
        })
        .collect::<Vec<Line>>();

    let total_row = Row::new(vec![
        Cell::new(format!("\nTotal")),
        Cell::new(format!("\n")),
        Cell::new(format!("\n")),
        Cell::new(format!("\n{:+.2}%", avg_today_variation)).style(avg_today_variation.style()),
        Cell::new(format!("\n{:+.2}%", avg_max_variation)).style(total_max_variation_style),
        Cell::new(format!("\n{:.03}%", avg_agent_share)).style(avg_agent_share.ustyle()),
        Cell::new(format!("\n${}", total_agent_stonk_value.format()))
            .style(total_agent_stonk_value.style()),
        Cell::new(total_market_cap_text).style(total_max_variation_style),
        Cell::new(top_portfolios),
    ])
    .style(Style::new().fg(colors.header_fg).bg(colors.header_bg))
    .height(3);

    rows.push(total_row);

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
            Constraint::Length(12),
            Constraint::Length(24),
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
    if ui_options.focus_on_stonk.is_some() {
        render_stonk(frame, market, agent, ui_options, area)?;
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
    market: &Market,
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
        Constraint::Max(CARD_HEIGHT / 2 + 2),
        Constraint::Length(2),
    ])
    .split(split[1].inner(&Margin {
        horizontal: 0,
        vertical: 1,
    }));

    let num_night_events = agent.available_night_events().len();

    if num_night_events > 0 {
        let cards_split =
            Layout::horizontal([Constraint::Length(CARD_WIDTH + 4)].repeat(MAX_EVENTS_PER_NIGHT))
                .split(v_split[2]);

        for i in 0..num_night_events {
            let cards = agent.available_night_events()[i].cards();
            // If there is not more than half of the time still available, skip the animation
            if counter < NIGHT_LENGTH / 2
                && ui_options.render_counter < ANIMATION_RATE * CARD_ANIMATION_FRAMES
            {
                frame.render_widget(
                    Paragraph::new(
                        cards[(ui_options.render_counter / ANIMATION_RATE) % CARD_ANIMATION_FRAMES]
                            .clone(),
                    ),
                    cards_split[i].inner(&Margin {
                        horizontal: 2,
                        vertical: 1,
                    }),
                );
            } else {
                let selected_event = agent.available_night_events()[i].clone();
                let border_style = if agent.selected_action().is_some() {
                    Style::default().green().on_green()
                } else {
                    Style::default().red().on_red()
                };
                if let Some(action) = agent.selected_action().cloned() {
                    if action == selected_event.action() {
                        frame.render_widget(
                            Paragraph::new(cards[CARD_ANIMATION_FRAMES - 1].clone())
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
                        frame.render_widget(
                            Paragraph::new(UNSELECTED_CARD.clone()),
                            cards_split[i].inner(&Margin {
                                horizontal: 2,
                                vertical: 1,
                            }),
                        );
                    }
                } else {
                    if ui_options.selected_event_card_index == i {
                        frame.render_widget(
                            Paragraph::new(cards[CARD_ANIMATION_FRAMES - 1].clone())
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
                        frame.render_widget(
                            Paragraph::new(cards[CARD_ANIMATION_FRAMES - 1].clone()),
                            cards_split[i].inner(&Margin {
                                horizontal: 2,
                                vertical: 1,
                            }),
                        );
                    }
                }

                let title_style = if agent.selected_action().is_some()
                    && ui_options.selected_event_card_index == i
                {
                    Style::default().green()
                } else {
                    Style::default().black()
                };
                let mut lines = vec![
                    Line::from(Span::styled(
                        selected_event.to_string().to_ascii_uppercase(),
                        title_style,
                    )),
                    Line::from(""),
                ];

                let description = selected_event.description(agent, market);
                for l in description.iter() {
                    lines.push(Line::from(l.as_str()).bold().black());
                }

                frame.render_widget(
                    Paragraph::new(lines).centered(),
                    cards_split[i].inner(&Margin {
                        horizontal: 3,
                        vertical: 3,
                    }),
                );
            }
        }

        if num_night_events < MAX_EVENTS_PER_NIGHT {
            frame.render_widget(
                Paragraph::new(format!("No more events available tonight. Unlock up to {MAX_EVENTS_PER_NIGHT} events by buying more stonks!")).centered().wrap(Wrap { trim: true }),
                cards_split[MAX_EVENTS_PER_NIGHT - 1].inner(&Margin {
                    horizontal: 1,
                    vertical: 2,
                }),
            );
        }
    } else {
        frame.render_widget(
            Paragraph::new(
                "No special events available tonight. Unlock events by buying more stonks!",
            )
            .centered()
            .wrap(Wrap { trim: true }),
            v_split[2],
        );
    }
    frame.render_widget(Paragraph::new(STONKS_LINES.clone()).centered(), v_split[0]);

    Ok(())
}

pub(crate) fn render_stonk(
    frame: &mut Frame,
    market: &Market,
    agent: &UserAgent,
    ui_options: &UiOptions,
    area: Rect,
) -> AppResult<()> {
    let stonk_id = ui_options
        .focus_on_stonk
        .expect("Focus_on_stonk should be some.");
    let stonk = &market.stonks[stonk_id];
    let styles = vec![
        Style::default().cyan(),
        Style::default().magenta(),
        Style::default().green(),
        Style::default().red(),
        Style::default().yellow(),
        Style::default().blue(),
        Style::default().white(),
        Style::default().light_green(),
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
        .marker(symbols::Marker::Braille)
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
        max_y_bound = max_price / 20 * 20 + 20 + max_price % 20;
    }

    let n_y_labels = area.height as usize / 6;
    let y_labels: Vec<Span<'static>> = (0..=n_y_labels)
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

    let stonk_info = if agent.has_condition(AgentCondition::UltraVision) {
        stonk.info(stonk.number_of_shares)
    } else {
        stonk.info(agent.owned_stonks()[stonk.id])
    };

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
                .title(stonk_info)
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
    let extra_text = match market.phase {
        GamePhase::Day { .. } => {
            if let Some(stonk_id) = ui_options.focus_on_stonk {
                let amount = agent.owned_stonks()[stonk_id];
                let stonk = &market.stonks[stonk_id];
                format!(
                    "Owned shares {} ({:.02}%) - value ${}",
                    amount,
                    stonk.to_stake(agent.owned_stonks()[stonk.id]) * 100.0,
                    (stonk.current_unit_price_cents() * amount).format()
                )
            } else {
                format!(
                    "{} player{} online - `ssh {}@frittura.org -p 3333`",
                    number_of_players,
                    if number_of_players > 1 { "s" } else { "" },
                    agent.username()
                )
            }
        }
        GamePhase::Night { .. } => {
            format!(
                "{} player{} online - `ssh {}@frittura.org -p 3333`",
                number_of_players,
                if number_of_players > 1 { "s" } else { "" },
                agent.username()
            )
        }
    };
    let header_text = format!(
        "{} - Cash: ${:<6.2} - {}",
        market.phase.formatted(),
        agent.cash_dollars(),
        extra_text,
    );

    frame.render_widget(Paragraph::new(header_text), area);
}

fn render_stonk_info(
    frame: &mut Frame,
    market: &Market,
    _agent: &UserAgent,
    ui_options: &UiOptions,
    area: Rect,
) {
    let stonk_id = if let Some(stonk_id) = ui_options.focus_on_stonk {
        stonk_id
    } else {
        ui_options.selected_stonk_index
    };
    let stonk = &market.stonks[stonk_id];
    frame.render_widget(
        Paragraph::new(stonk.description.clone()).wrap(Wrap { trim: true }),
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
            if let Some(_) = ui_options.focus_on_stonk {
                lines.push(
                    format!(
                        "{:28} {:28} {:28}",
                        "`↑↓`:select stonk", "`return`:main table", "`z`:zoom level",
                    )
                    .into(),
                );
            } else {
                lines.push(
                    format!("{:28} {:28}", "`↑↓`:select stonk", "`return`:show graph",).into(),
                );
            }

            let stonk_id = if let Some(stonk_id) = ui_options.focus_on_stonk {
                stonk_id
            } else {
                ui_options.selected_stonk_index
            };
            let stonk = &market.stonks[stonk_id];
            let max_buy_amount = stonk.max_buy_amount(agent.cash());

            lines.push(
                format!(
                    "{:28} {:28} {:28}",
                    format!(
                        "`b`: buy  x{} (${})",
                        1.min(max_buy_amount),
                        stonk.buy_price_cents(1.min(max_buy_amount)).format()
                    ),
                    format!(
                        "`B`: buy  x{} (${})",
                        100.min(max_buy_amount),
                        stonk.buy_price_cents(100.min(max_buy_amount)).format()
                    ),
                    format!(
                        "`m`: buy  x{} (${})",
                        max_buy_amount,
                        stonk.buy_price_cents(max_buy_amount).format()
                    ),
                )
                .into(),
            );
            let owned_amount = agent.owned_stonks()[stonk.id];
            lines.push(
                format!(
                    "{:28} {:28} {:28}",
                    format!(
                        "`s`: sell x{} (${})",
                        1.min(owned_amount),
                        stonk.sell_price_cents(1.min(owned_amount)).format()
                    ),
                    format!(
                        "`S`: sell x{} (${})",
                        100.min(owned_amount),
                        stonk.sell_price_cents(100.min(owned_amount)).format()
                    ),
                    format!(
                        "`d`: sell x{} (${})",
                        owned_amount,
                        stonk.sell_price_cents(owned_amount).format()
                    ),
                )
                .into(),
            );
        }
        GamePhase::Night { .. } => {
            if let Some(action) = agent.selected_action().cloned() {
                for event in agent.available_night_events().iter() {
                    if event.action() == action {
                        lines.push(format!("You selected `{}`", event).into());
                    }
                }
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
        Constraint::Length(1), //header
        Constraint::Min(0),    //body
        Constraint::Length(3), //footer
    ])
    .split(area);

    render_header(
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
            GamePhase::Day { .. } => {
                let sub_split = Layout::vertical([
                    Constraint::Min(0),    //body
                    Constraint::Length(3), // stonk info / newspaper
                ])
                .split(split[1]);
                render_day(frame, market, agent, ui_options, sub_split[0])?;
                render_stonk_info(frame, market, agent, ui_options, sub_split[1]);
            }
            GamePhase::Night { counter, .. } => {
                render_night(frame, market, counter, agent, ui_options, split[1])?
            }
        },
    }

    render_footer(frame, market, agent, ui_options, split[2]);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ANIMATION_RATE, CARD_WIDTH, DOGE_CARDS, ELON_CARDS, KIM_CARDS, STONKS_CARDS};
    use crate::utils::AppResult;
    use ratatui::{
        backend::CrosstermBackend,
        layout::{Layout, Margin},
        widgets::Paragraph,
        Terminal,
    };
    use std::{thread, time::Duration};

    #[test]
    fn test_card_animation() -> AppResult<()> {
        // create crossterm terminal to stdout
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend).unwrap();

        let mut idx = 0;

        let stonks = STONKS_CARDS.clone();
        let doge = DOGE_CARDS.clone();
        let elon = ELON_CARDS.clone();
        let kim = KIM_CARDS.clone();

        terminal.clear()?;
        loop {
            if idx == stonks.len() * ANIMATION_RATE {
                break;
            }

            thread::sleep(Duration::from_millis(50));
            terminal.draw(|frame| {
                let area = frame.size();

                let split = Layout::horizontal([CARD_WIDTH + 2].repeat(4)).split(area);
                frame.render_widget(
                    Paragraph::new(stonks[idx / ANIMATION_RATE].clone()),
                    split[0].inner(&Margin {
                        horizontal: 1,
                        vertical: 1,
                    }),
                );
                frame.render_widget(
                    Paragraph::new(doge[idx / ANIMATION_RATE].clone()),
                    split[1].inner(&Margin {
                        horizontal: 1,
                        vertical: 1,
                    }),
                );
                frame.render_widget(
                    Paragraph::new(elon[idx / ANIMATION_RATE].clone()),
                    split[2].inner(&Margin {
                        horizontal: 1,
                        vertical: 1,
                    }),
                );
                frame.render_widget(
                    Paragraph::new(kim[idx / ANIMATION_RATE ].clone()),
                    split[3].inner(&Margin {
                        horizontal: 1,
                        vertical: 1,
                    }),
                );
            })?;

            idx += 1;
        }

        Ok(())
    }
}

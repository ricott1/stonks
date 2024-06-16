use crate::agent::{DecisionAgent, UserAgent};
use crate::stonk::{GamePhase, Market, Stonk};
use crate::utils::{img_to_lines, AppResult};
use crossterm::event::KeyCode;
use ratatui::layout::Constraint;
use ratatui::style::palette::tailwind;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Axis, Block, Cell, Chart, Dataset, GraphType, HighlightSpacing, Paragraph, Row, Table,
    TableState,
};
use ratatui::{layout::Layout, Frame};

const STONKS: [&'static str; 6] = [
    "███████╗████████╗ ██████╗ ███╗   ██╗██╗  ██╗███████╗██╗",
    "██╔════╝╚══██╔══╝██╔═══██╗████╗  ██║██║ ██╔╝██╔════╝██║",
    "███████╗   ██║   ██║   ██║██╔██╗ ██║█████╔╝ ███████╗██║",
    "╚════██║   ██║   ██║   ██║██║╚██╗██║██╔═██╗ ╚════██║╚═╝",
    "███████║   ██║   ╚██████╔╝██║ ╚████║██║  ██╗███████║██╗",
    "╚══════╝   ╚═╝    ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝╚══════╝╚═╝",
];

const PALETTES: [tailwind::Palette; 4] = [
    tailwind::BLUE,
    tailwind::EMERALD,
    tailwind::INDIGO,
    tailwind::RED,
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
            alt_row_color: tailwind::SLATE.c900,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UiDisplay {
    Stonks,
    Portfolio,
}
#[derive(Debug, Clone, Copy)]
pub struct UiOptions {
    min_y_bound_offset: i32,
    bound_spread: u8,
    pub focus_on_stonk: Option<usize>,
    display: UiDisplay,
    selected_stonk_index: usize,
    palette_index: usize,
}

impl UiOptions {
    pub fn new() -> Self {
        UiOptions {
            min_y_bound_offset: 0,
            bound_spread: 0,
            focus_on_stonk: None,
            display: UiDisplay::Stonks,
            selected_stonk_index: 0,
            palette_index: 0,
        }
    }

    pub fn handle_key_events(&mut self, key_code: KeyCode) -> AppResult<()> {
        match key_code {
            crossterm::event::KeyCode::Down => {
                if let Some(_) = self.focus_on_stonk {
                    self.min_y_bound_offset -= 10
                } else {
                    self.selected_stonk_index = (self.selected_stonk_index + 1) % 8;
                }
            }

            crossterm::event::KeyCode::Up => {
                if let Some(_) = self.focus_on_stonk {
                    self.min_y_bound_offset += 10
                } else {
                    self.selected_stonk_index = (self.selected_stonk_index + 8 - 1) % 8;
                }
            }

            crossterm::event::KeyCode::Left => {
                self.bound_spread = self.bound_spread.checked_sub(1).unwrap_or(0)
            }

            crossterm::event::KeyCode::Right => {
                self.bound_spread = self.bound_spread.checked_add(1).unwrap_or(u8::MAX)
            }

            crossterm::event::KeyCode::Enter => {
                if let Some(_) = self.focus_on_stonk {
                    self.reset();
                } else {
                    let idx = self.selected_stonk_index;
                    self.reset();
                    self.focus_on_stonk = Some(idx);
                }
            }

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
        self.min_y_bound_offset = 0;
        self.bound_spread = 0;
        self.selected_stonk_index = 0;
    }
}

fn build_stonks_table<'a>(market: &Market, colors: TableColors) -> Table<'a> {
    let header_style = Style::default().fg(colors.header_fg).bg(colors.header_bg);
    let selected_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .fg(colors.selected_style_fg);

    let header = ["Name", "Last buy", "Last sell"]
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
        [
            stonk.name.clone(),
            format!("${:.2}", stonk.formatted_buy_price()),
            format!("${:.2}", stonk.formatted_sell_price()),
        ]
        .into_iter()
        .map(|content| Cell::from(Text::from(format!("\n{content}"))))
        .collect::<Row>()
        .style(Style::new().fg(colors.row_fg).bg(color))
        .height(3)
    });
    let bar = " █ ";
    Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .highlight_style(selected_style)
    .highlight_symbol(Text::from(vec!["".into(), bar.into(), "".into()]))
    .bg(colors.buffer_bg)
    .highlight_spacing(HighlightSpacing::Always)
}

fn render_day(
    frame: &mut Frame,
    market: &Market,
    ui_options: UiOptions,
    agent: &UserAgent,
) -> AppResult<()> {
    if let Some(stonk_id) = ui_options.focus_on_stonk {
        let stonk = &market.stonks[stonk_id];
        render_stonk(frame, market, ui_options, agent, stonk)?;
    } else {
        let colors = TableColors::new(&PALETTES[ui_options.palette_index]);
        let table = build_stonks_table(market, colors);
        frame.render_stateful_widget(
            table,
            frame.size(),
            &mut TableState::default().with_selected(Some(ui_options.selected_stonk_index)),
        );
    }
    Ok(())
}

fn render_stonk(
    frame: &mut Frame,
    market: &Market,
    ui_options: UiOptions,
    agent: &UserAgent,
    stonk: &Stonk,
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
        Style::default().white(),
        Style::default().light_magenta(),
    ];

    let mut x_ticks = market.x_ticks();

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

    let datas = vec![stonk.data(x_ticks.clone())];

    let datasets = vec![Dataset::default()
        .graph_type(GraphType::Line)
        .name(format!("{}: {}", stonk.id + 1, stonk.name.clone()))
        .marker(symbols::Marker::HalfBlock)
        .style(styles[stonk.id])
        .data(&datas[0])];

    let mut min_y_bound;
    let mut max_y_bound;

    let min_price = datas[0]
        .iter()
        .map(|(_, d)| *d as u64)
        .min()
        .unwrap_or_default();
    let max_price = datas[0]
        .iter()
        .map(|(_, d)| *d as u64)
        .max()
        .unwrap_or_default();

    if min_price < 20 {
        min_y_bound = 0;
    } else {
        min_y_bound = min_price as u16 / 20 * 20 - 20;
    }
    if max_price < 20 {
        max_y_bound = 40;
    } else {
        max_y_bound = max_price as u16 / 20 * 20 + 20;
    }

    if ui_options.min_y_bound_offset >= 0 {
        min_y_bound += ui_options.min_y_bound_offset as u16;
        max_y_bound += ui_options.min_y_bound_offset as u16;
    } else {
        let offset = ((-ui_options.min_y_bound_offset) as u16).min(min_y_bound);
        min_y_bound -= offset;
        max_y_bound -= offset;
    }

    let spread = (1.1_f64).powf(ui_options.bound_spread as f64);

    min_y_bound = (min_y_bound as f64 / spread) as u16;
    max_y_bound = (max_y_bound as f64 * spread) as u16;

    println!("min bound {}, max bound {}", min_y_bound, max_y_bound);

    let mut labels: Vec<Span<'static>> = vec![];

    let stonk_price = (stonk.price_per_share_in_cents as f64 / 100.0) as u16;

    for r in 0..=4 {
        labels.push(
            (min_y_bound + r * (max_y_bound - min_y_bound) / 4)
                .to_string()
                .bold(),
        );
        if (min_y_bound + r * (max_y_bound - min_y_bound) / 4) < stonk_price
            && stonk_price < (min_y_bound + (r + 1) * (max_y_bound - min_y_bound) / 4)
        {
            labels.push(Span::raw(format!("{:.0}", stonk_price)))
        }
    }

    let chart = Chart::new(datasets)
        .block(Block::bordered().title(format!(" Stonk Market: {:?} ", market.phase).cyan().bold()))
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
                .labels(labels)
                .bounds([min_y_bound as f64, max_y_bound as f64]),
        );

    frame.render_widget(chart, split[0]);

    frame.render_widget(
        Paragraph::new(format!("'#':select stonk number '#', enter:reset",)),
        split[1],
    );

    frame.render_widget(
        Paragraph::new(format!(
            "b: buy for ${:.2}  s: sell for ${:.2}",
            stonk.formatted_buy_price(),
            stonk.formatted_sell_price(),
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

pub fn render(
    frame: &mut Frame,
    market: &Market,
    ui_options: UiOptions,
    agent: &UserAgent,
    number_of_players: usize,
) -> AppResult<()> {
    clear(frame);

    match ui_options.display {
        UiDisplay::Portfolio => {}
        UiDisplay::Stonks => match market.phase {
            GamePhase::Day { .. } => render_day(frame, market, ui_options, agent)?,
            GamePhase::Night { counter } => {
                let area = frame.size();
                let img_width = 2 * STONKS.len() as u16;
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

                let stonks = img_to_lines("stonk.png").expect("Cannot load stonk image");
                frame.render_widget(Paragraph::new(stonks), v_split[0]);
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

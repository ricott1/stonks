use crate::stonk::{App, GamePhase};
use crate::utils::{img_to_lines, AppResult};
use ratatui::layout::Constraint;
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::text::Line;
use ratatui::widgets::{Axis, Block, Chart, Dataset, Paragraph};
use ratatui::{layout::Layout, Frame};

const STONKS: [&'static str; 6] = [
    "███████╗████████╗ ██████╗ ███╗   ██╗██╗  ██╗███████╗██╗",
    "██╔════╝╚══██╔══╝██╔═══██╗████╗  ██║██║ ██╔╝██╔════╝██║",
    "███████╗   ██║   ██║   ██║██╔██╗ ██║█████╔╝ ███████╗██║",
    "╚════██║   ██║   ██║   ██║██║╚██╗██║██╔═██╗ ╚════██║╚═╝",
    "███████║   ██║   ╚██████╔╝██║ ╚████║██║  ██╗███████║██╗",
    "╚══════╝   ╚═╝    ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝╚══════╝╚═╝",
];

pub struct Ui<'a> {
    pub min_y_bound: u16,
    pub max_y_bound: u16,
    stonk: Vec<Line<'a>>,
}

impl<'a> Ui<'a> {
    fn render_day(&mut self, frame: &mut Frame, app: &App) -> AppResult<()> {
        let area = frame.size();
        let split = Layout::vertical([0, 60]).split(area);

        let mut x_ticks = app.x_ticks();

        let data_size = split[1].width as usize - 5;
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

        let datas = app
            .stonks
            .iter()
            .map(|stonk| stonk.data(x_ticks.clone()))
            .collect::<Vec<Vec<(f64, f64)>>>();

        // let avg_datas = app
        //     .stonks
        //     .iter()
        //     .map(|stonk| stonk.avg_data(x_ticks.clone()))
        //     .collect::<Vec<Vec<(f64, f64)>>>();

        let mut datasets = vec![];

        // for idx in 0..datas.len() {
        //     datasets.push(
        //         Dataset::default()
        //             .marker(symbols::Marker::Dot)
        //             .white()
        //             .style(styles[idx])
        //             .data(&avg_datas[idx]),
        //     );
        // }

        for idx in 0..datas.len() {
            datasets.push(
                Dataset::default()
                    .name(app.stonks[idx].name.clone())
                    .marker(symbols::Marker::HalfBlock)
                    .style(app.stonks[idx].class.style())
                    .data(&datas[idx]),
            );
        }

        let avg_bound = (self.min_y_bound + self.max_y_bound) / 2;

        let chart = Chart::new(datasets)
            .block(
                Block::bordered().title(format!(" Stonk Market: {:?} ", app.phase).cyan().bold()),
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
                        self.min_y_bound.to_string().bold(),
                        avg_bound.to_string().bold(),
                        self.max_y_bound.to_string().bold(),
                    ])
                    .bounds([self.min_y_bound as f64, self.max_y_bound as f64]),
            );

        frame.render_widget(chart, split[1]);
        Ok(())
    }

    pub fn new() -> Self {
        Self {
            min_y_bound: 40,
            max_y_bound: 140,
            stonk: img_to_lines("stonk.png").expect("Cannot load stonk image"),
        }
    }

    // pub fn handle_key_events(
    //     &self,
    //     key_event: crossterm::event::KeyEvent,
    // ) -> Option<UiCallbackPreset> {
    // }

    // pub fn handle_mouse_events(
    //     &self,
    //     mouse_event: crossterm::event::MouseEvent,
    // ) -> Option<UiCallbackPreset> {
    // }
    fn clear(&self, frame: &mut Frame) {
        let area = frame.size();
        let mut lines = vec![];
        for _ in 0..area.height {
            lines.push(Line::from(" ".repeat(area.width.into())));
        }
        let clear = Paragraph::new(lines).style(Color::White);
        frame.render_widget(clear, area);
    }

    pub fn render(&mut self, frame: &mut Frame, app: &App) -> AppResult<()> {
        self.clear(frame);
        match app.phase {
            GamePhase::Day { .. } => self.render_day(frame, app)?,
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
                    v_split[1],
                );
            }
        }

        Ok(())
    }
}

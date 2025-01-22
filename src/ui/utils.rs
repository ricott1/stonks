use crate::game::events::{EventRarity, NightEvent};
use crate::utils::*;
use image::RgbaImage;
use once_cell::sync::Lazy;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub const CARD_WIDTH: u16 = 30;
pub const CARD_HEIGHT: u16 = 40;
const NO_ANIMATION_FRAMES: usize = 6;
const CARD_ANIMATION_FRAMES: usize = NO_ANIMATION_FRAMES + CARD_WIDTH as usize + 1;

static STONKS_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| image_to_cards("images/stonks.png"));
static DOGE_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| image_to_cards("images/doge.png"));
static ELON_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| image_to_cards("images/elon.png"));

#[allow(dead_code)]
static KIM_CARDS: Lazy<Vec<Vec<Line>>> = Lazy::new(|| image_to_cards("images/kim.png"));

pub static UNSELECTED_CARD: Lazy<Vec<Line>> = Lazy::new(|| {
    let image = read_image("images/unselected_card.png").expect("Cannot load image from file");
    image_to_lines(&image)
});

pub trait Carded {
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

pub fn image_to_lines<'a>(img: &RgbaImage) -> Vec<Line<'a>> {
    let mut lines: Vec<Line> = vec![];
    let width = img.width();
    let height = img.height();

    for y in (0..height - 1).step_by(2) {
        let mut line: Vec<Span> = vec![];

        for x in 0..width {
            let top_pixel = img.get_pixel(x, y);
            let btm_pixel = img.get_pixel(x, y + 1);

            // both pixels are transparent
            if top_pixel[3] == 0 && btm_pixel[3] == 0 {
                line.push(Span::raw(" "));
                continue;
            }

            // render top pixel
            if top_pixel[3] > 0 && btm_pixel[3] == 0 {
                let [r, g, b, _] = top_pixel.0;
                let color = Color::Rgb(r, g, b);
                line.push(Span::styled("▀", Style::default().fg(color)));
                continue;
            }

            // render bottom pixel
            if top_pixel[3] == 0 && btm_pixel[3] > 0 {
                let [r, g, b, _] = btm_pixel.0;
                let color = Color::Rgb(r, g, b);
                line.push(Span::styled("▄", Style::default().fg(color)));
                continue;
            }

            // render both pixels
            let [fr, fg, fb, _] = top_pixel.0;
            let fg_color = Color::Rgb(fr, fg, fb);
            let [br, bg, bb, _] = btm_pixel.0;
            let bg_color = Color::Rgb(br, bg, bb);
            line.push(Span::styled(
                "▀",
                Style::default().fg(fg_color).bg(bg_color),
            ));
        }
        lines.push(Line::from(line));
    }
    // append last line if height is odd
    if height % 2 == 1 {
        let mut line: Vec<Span> = vec![];
        for x in 0..width {
            let top_pixel = img.get_pixel(x, height - 1);
            if top_pixel[3] == 0 {
                line.push(Span::raw(" "));
                continue;
            }
            let [r, g, b, _] = top_pixel.0;
            let color = Color::Rgb(r, g, b);
            line.push(Span::styled("▀", Style::default().fg(color)));
        }
        lines.push(Line::from(line));
    }

    lines
}

pub fn image_to_cards(path: &str) -> Vec<Vec<Line>> {
    let back_image = read_image(path).expect("Cannot load image from file");
    let back_lines = image_to_lines(&back_image);
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
                    resize_image(&back_image, nwidth, nheight).expect("Should resize image");
                let lines = image_to_lines(&resized_image);

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
                    resize_image(&front_image, nwidth, nheight).expect("Should resize image");
                let lines = image_to_lines(&resized_image);

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
            image_to_lines(&front_image)
        })
        .collect::<Vec<Vec<Line>>>()
}

#[cfg(test)]
mod tests {
    use super::CARD_WIDTH;
    use crate::ui::utils::*;
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
        const ANIMATION_RATE: usize = 2;
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
                let area = frame.area();

                let split = Layout::horizontal([CARD_WIDTH + 2].repeat(4)).split(area);
                frame.render_widget(
                    Paragraph::new(stonks[idx / ANIMATION_RATE].clone()),
                    split[0].inner(Margin {
                        horizontal: 1,
                        vertical: 1,
                    }),
                );
                frame.render_widget(
                    Paragraph::new(doge[idx / ANIMATION_RATE].clone()),
                    split[1].inner(Margin {
                        horizontal: 1,
                        vertical: 1,
                    }),
                );
                frame.render_widget(
                    Paragraph::new(elon[idx / ANIMATION_RATE].clone()),
                    split[2].inner(Margin {
                        horizontal: 1,
                        vertical: 1,
                    }),
                );
                frame.render_widget(
                    Paragraph::new(kim[idx / ANIMATION_RATE].clone()),
                    split[3].inner(Margin {
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

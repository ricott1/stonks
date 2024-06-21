use crate::market::Market;
use crate::ssh_server::AgentsDatabase;
use image::io::Reader as ImageReader;
use image::{Pixel, RgbaImage};
use include_dir::{include_dir, Dir};
use ratatui::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;

pub type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

static ASSETS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/");
static AGENTS_STORE_FILENAME: &'static str = "agents.json";
static MARKET_STORE_FILENAME: &'static str = "market.json";

fn read_image(path: &str) -> AppResult<RgbaImage> {
    let file = ASSETS_DIR.get_file(path);
    if file.is_none() {
        return Err(format!("File {} not found", path).into());
    }
    let img = ImageReader::new(Cursor::new(file.unwrap().contents()))
        .with_guessed_format()?
        .decode()?
        .into_rgba8();
    Ok(img)
}

pub fn img_to_lines<'a>(path: &str) -> AppResult<Vec<Line<'a>>> {
    let img = read_image(path)?;
    let mut lines: Vec<Line> = vec![];
    let width = img.width();
    let height = img.height();

    for y in (0..height - 1).step_by(2) {
        let mut line: Vec<Span> = vec![];

        for x in 0..width {
            let top_pixel = img.get_pixel(x, y).to_rgba();
            let btm_pixel = img.get_pixel(x, y + 1).to_rgba();
            if top_pixel[3] == 0 && btm_pixel[3] == 0 {
                line.push(Span::raw(" "));
                continue;
            }

            if top_pixel[3] > 0 && btm_pixel[3] == 0 {
                let [r, g, b, _] = top_pixel.0;
                let color = Color::Rgb(r, g, b);
                line.push(Span::styled("▀", Style::default().fg(color)));
            } else if top_pixel[3] == 0 && btm_pixel[3] > 0 {
                let [r, g, b, _] = btm_pixel.0;
                let color = Color::Rgb(r, g, b);
                line.push(Span::styled("▄", Style::default().fg(color)));
            } else {
                let [fr, fg, fb, _] = top_pixel.0;
                let fg_color = Color::Rgb(fr, fg, fb);
                let [br, bg, bb, _] = btm_pixel.0;
                let bg_color = Color::Rgb(br, bg, bb);
                line.push(Span::styled(
                    "▀",
                    Style::default().fg(fg_color).bg(bg_color),
                ));
            }
        }
        lines.push(Line::from(line));
    }
    // append last line if height is odd
    if height % 2 == 1 {
        let mut line: Vec<Span> = vec![];
        for x in 0..width {
            let top_pixel = img.get_pixel(x, height - 1).to_rgba();
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

    Ok(lines)
}

fn store_path(filename: &str) -> AppResult<PathBuf> {
    let dirs = directories::ProjectDirs::from("org", "frittura", "stonks")
        .ok_or("Failed to get directories")?;
    let config_dirs = dirs.config_dir();
    if !config_dirs.exists() {
        std::fs::create_dir_all(config_dirs)?;
    }
    let path = config_dirs.join(filename);
    Ok(path)
}

fn save_to_json<T: Serialize>(path: PathBuf, data: &T) -> AppResult<()> {
    let file = File::create(path)?;
    assert!(file.metadata()?.is_file());
    let buffer = std::io::BufWriter::new(file);
    serde_json::to_writer(buffer, data)?;
    Ok(())
}

fn load_from_json<T: for<'a> Deserialize<'a>>(path: PathBuf) -> AppResult<T> {
    let file = File::open(path)?;
    let data: T = serde_json::from_reader(file)?;
    Ok(data)
}

pub fn save_agents(agents: &AgentsDatabase) -> AppResult<()> {
    save_to_json(store_path(AGENTS_STORE_FILENAME)?, agents)?;
    Ok(())
}

pub fn save_market(market: &Market) -> AppResult<()> {
    save_to_json(store_path(MARKET_STORE_FILENAME)?, market)?;
    Ok(())
}

pub fn load_agents() -> AppResult<AgentsDatabase> {
    load_from_json(store_path(AGENTS_STORE_FILENAME)?)
}

pub fn load_market() -> AppResult<Market> {
    load_from_json(store_path(MARKET_STORE_FILENAME)?)
}

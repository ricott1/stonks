use crate::game::agent::UserAgent;
use crate::game::market::{Market, NUMBER_OF_STONKS};
use crate::game::stonk::Stonk;
use anyhow::anyhow;
use image::imageops::resize;
use image::ImageReader;
use image::RgbaImage;
use include_dir::{include_dir, Dir};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;

pub type AppResult<T> = Result<T, anyhow::Error>;
pub type AgentId = uuid::Uuid;

pub fn fresh_chacha_rng() -> ChaCha8Rng {
    ChaCha8Rng::from_rng(&mut rand::rng())
}

static ASSETS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/");
static MARKET_STORE_FILENAME: &str = "market.json";

pub fn read_image(path: &str) -> AppResult<RgbaImage> {
    let file = ASSETS_DIR.get_file(path);
    if file.is_none() {
        return Err(anyhow!("File {} not found", path));
    }
    let img = ImageReader::new(Cursor::new(file.unwrap().contents()))
        .with_guessed_format()?
        .decode()?
        .into_rgba8();
    Ok(img)
}

pub fn resize_image(image: &RgbaImage, nwidth: u32, nheight: u32) -> AppResult<RgbaImage> {
    Ok(resize(
        image,
        nwidth,
        nheight,
        image::imageops::FilterType::Triangle,
    ))
}

pub fn store_path(filename: &str) -> AppResult<PathBuf> {
    let dirs = directories::ProjectDirs::from("org", "frittura", "stonks")
        .ok_or(anyhow!("Failed to get directories"))?;
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

pub fn save_agent(agent: &UserAgent) -> AppResult<()> {
    save_to_json(
        store_path(format!("agent_{}.json", agent.username()).as_str())?,
        agent,
    )?;
    Ok(())
}

pub fn delete_all_data() -> AppResult<()> {
    let dirs = directories::ProjectDirs::from("org", "frittura", "stonks")
        .ok_or(anyhow!("Failed to get directories"))?;
    let config_dirs = dirs.config_dir();
    if !config_dirs.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(config_dirs)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub fn save_market(market: &Market) -> AppResult<()> {
    save_to_json(store_path(MARKET_STORE_FILENAME)?, market)?;
    Ok(())
}

pub fn load_agent(username: &str) -> AppResult<UserAgent> {
    load_from_json(store_path(format!("agent_{}.json", username).as_str())?)
}

pub fn load_market() -> AppResult<Market> {
    load_from_json(store_path(MARKET_STORE_FILENAME)?)
}

pub fn load_stonks_data() -> AppResult<[Stonk; NUMBER_OF_STONKS]> {
    let file = ASSETS_DIR
        .get_file("data/stonks_data.json")
        .expect("Failed to get stonks data file");
    let data = file
        .contents_utf8()
        .expect("Failed to read stonks data file");
    let stonks = serde_json::from_str(data)?;
    Ok(stonks)
}

#[cfg(test)]
mod tests {
    use super::{save_agent, AppResult};
    use crate::game::agent::UserAgent;
    use directories;
    use std::fs::File;

    #[test]
    fn test_path() {
        let dirs = directories::ProjectDirs::from("org", "frittura", "test");
        assert!(dirs.is_some());
        let dirs_ok = dirs.unwrap();
        let config_dirs = dirs_ok.config_dir();
        println!("{:?}", config_dirs);
        if !config_dirs.exists() {
            std::fs::create_dir_all(config_dirs).unwrap();
        }
        let path = config_dirs.join("test");
        let file = File::create(path.clone());
        assert!(file.is_ok());
        assert!(path.is_file());
        if config_dirs.exists() {
            std::fs::remove_dir_all(config_dirs).unwrap();
        }
    }

    #[test]
    fn test_save() -> AppResult<()> {
        let agents = vec![
            UserAgent::new("username".into(), [0; 32]),
            UserAgent::new("username2".into(), [0; 32]),
        ];

        save_agent(&agents[0])?;

        Ok(())
    }
}

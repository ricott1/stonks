use stonks::{
    ssh_server::AppServer,
    stonk::{App, Stonk, StonkClass},
    utils::AppResult,
};
use tracing::metadata::LevelFilter;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::default().add_directive(LevelFilter::OFF.into()))
        .with_line_number(true)
        .with_file(true)
        .init();

    let cassius_inc = Stonk::new(
        0,
        StonkClass::Technology,
        "Cassius INC".into(),
        98.0,
        2500,
        0.01,
        0.025,
    );
    let tesla = Stonk::new(
        1,
        StonkClass::Technology,
        "Tesla".into(),
        100.0,
        250,
        0.0,
        0.01,
    );
    let rovanti = Stonk::new(
        1,
        StonkClass::Commodity,
        "Rovanti".into(),
        80.0,
        250,
        0.005,
        0.005,
    );
    let riccardino = Stonk::new(
        1,
        StonkClass::Technology,
        "Riccardino".into(),
        90.0,
        10000,
        0.000,
        0.01,
    );

    let stonks = vec![tesla, cassius_inc, rovanti, riccardino];
    let app = App::new(stonks);

    AppServer::new(app).run().await?;

    Ok(())
}

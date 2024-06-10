use stonks::{
    ssh_server::AppServer,
    stonk::{App, StonkClass},
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

    let mut app = App::new();

    app.new_stonk(
        StonkClass::Technology,
        "Cassius INC".into(),
        98.0,
        2500,
        0.01,
        0.025,
    );
    app.new_stonk(
        StonkClass::Technology,
        "Tesla".into(),
        100.0,
        250,
        0.0,
        0.01,
    );
    app.new_stonk(
        StonkClass::Commodity,
        "Rovanti".into(),
        80.0,
        250,
        0.005,
        0.005,
    );
    app.new_stonk(
        StonkClass::Technology,
        "Riccardino".into(),
        90.0,
        10000,
        0.000,
        0.01,
    );

    println!("Started App with {} stonks!", app.stonks.len());

    AppServer::new(app).run().await?;

    Ok(())
}

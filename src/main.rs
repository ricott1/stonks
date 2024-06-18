use stonks::{ssh_server::AppServer, stonk::Market, utils::AppResult};
use tracing::{debug, metadata::LevelFilter};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::default().add_directive(LevelFilter::OFF.into()))
        .with_line_number(true)
        .with_file(true)
        .init();

    let market = Market::new();
    debug!("Started Market with {} stonks!", market.stonks.len());

    AppServer::new(market).run().await?;

    Ok(())
}

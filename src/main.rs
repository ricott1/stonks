use clap::{ArgAction, Parser};
use stonks::{ssh_server::AppServer, utils::AppResult};
use tracing::metadata::LevelFilter;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[clap(name="Stonks", about = "Get rich or stonk tryin'", author, version, long_about = None)]
struct Args {
    #[clap(long, short = 's', action=ArgAction::Set, help = "Set random seed")]
    seed: Option<u64>,
    #[clap(long, short='r', action=ArgAction::SetTrue, help = "Reset storage")]
    reset: bool,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::default().add_directive(LevelFilter::INFO.into()))
        .with_line_number(true)
        .with_file(true)
        .init();

    let args = Args::parse();

    AppServer::new(args.reset, args.seed)?.run().await?;

    Ok(())
}

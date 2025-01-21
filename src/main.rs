use clap::{ArgAction, Parser};
use log::LevelFilter;
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    encode::pattern::PatternEncoder,
    Config,
};
use stonks::{
    ssh::AppServer,
    utils::{store_path, AppResult},
};

const DEFAULT_SERVER_SSH_PORT: u16 = 3333;

#[derive(Parser, Debug)]
#[clap(name="Stonks", about = "Get rich or stonk tryin'", author, version, long_about = None)]
struct Args {
    #[clap(long, short = 's', action=ArgAction::Set, help = "Set random seed")]
    seed: Option<u64>,
    #[clap(long, short = 'p', action=ArgAction::Set, help = "Set SSH server port")]
    port: Option<u16>,
    #[clap(long, short='r', action=ArgAction::SetTrue, help = "Reset storage")]
    reset: bool,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let logfile_path = store_path("minotaur.log")?;
    let logfile = FileAppender::builder()
        .append(false)
        .encoder(Box::new(PatternEncoder::new("{l} - {m}\n")))
        .build(logfile_path)?;

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .build(Root::builder().appender("logfile").build(LevelFilter::Info))?;

    log4rs::init_config(config)?;

    let args = Args::parse();
    let port = args.port.unwrap_or(DEFAULT_SERVER_SSH_PORT);
    let mut game_server = AppServer::new(port)?;
    game_server.run(args.reset, args.seed).await?;

    Ok(())
}

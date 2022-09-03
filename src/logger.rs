use std::error::Error;
use log::LevelFilter;
use log4rs::{
    append::{
        console::ConsoleAppender,
        file::FileAppender
    },
    encode::pattern::PatternEncoder,
    config::{
        Appender,
        Config,
        Logger,
        Root
    }
};

pub fn configure_logger() -> Result<Config, Box<dyn Error>> {
    let stdout = ConsoleAppender::builder().build();

    let requests = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d} - {m}{n}")))
        .build("logfiles/log");

    if let Err(inner) = requests {
        return Err(Box::new(inner));
    }
    let requests = requests.unwrap();

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("requests", Box::new(requests)))
        .logger(Logger::builder().build("app::*", LevelFilter::Info))
        .logger(Logger::builder()
            .appender("requests")
            .additive(false)
            .build("app:requests", LevelFilter::Info)
        ).build(Root::builder().appender("stdout").build(LevelFilter::Warn));

    if let Err(inner) = config {
        return Err(Box::new(inner));
    }

    Ok(config.unwrap())
}


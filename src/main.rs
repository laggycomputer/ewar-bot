mod handler;
mod util;
mod commands;

use crate::commands::meta::ping;
use clap::ValueHint;
use poise::FrameworkOptions;
use serenity::all::GatewayIntents;
use serenity::all::GuildId;
use serenity::Client;
use std::fs;
use std::path::PathBuf;
use yaml_rust2::YamlLoader;

struct BotVars {}

#[tokio::main]
async fn main() {
    let cmd = clap::command!("ewar-bot")
        .about("Discord bot for handling ranked Egyptian War backed by TrueSkill")
        .arg(clap::arg!(<"config"> "config file path")
            .value_parser(clap::value_parser!(PathBuf))
            .value_hint(ValueHint::FilePath));

    let args = cmd.get_matches();

    let config_path = args.get_one::<PathBuf>("config").expect("config file is bad path?");
    let config_doc = &YamlLoader::load_from_str(
        &*fs::read_to_string(config_path).expect("can't open config file")
    ).expect("can't parse config file")[0];

    let framework = poise::Framework::<BotVars, BotError>::builder()
        .options(FrameworkOptions {
            commands: vec![ping()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                println!("registered commands globally");
                poise::builtins::register_in_guild(ctx, &framework.options().commands, GuildId::from(576979941304827954)).await?;
                println!("registered commands locally");
                Ok(BotVars {})
            })
        })
        .build();

    let token = config_doc["token"].as_str().expect("discord client token missing");
    let mut client = Client::builder(&token, GatewayIntents::all())
        .event_handler(handler::EWarBotHandler)
        .framework(framework)
        .await
        .expect("couldn't make client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}

type BotError = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, BotVars, BotError>;
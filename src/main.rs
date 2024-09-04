mod handler;
mod util;
mod commands;

use crate::commands::meta::ping;
use clap::ValueHint;
use itertools::Itertools;
use pluralizer::pluralize;
use poise::{FrameworkOptions, PrefixFrameworkOptions};
use serenity::all::GatewayIntents;
use serenity::all::GuildId;
use serenity::Client;
use std::default::Default;
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

    let register_globally = config_doc["register_commands"]["global"].as_bool().expect("bad config spec");
    let guilds_to_register_in = if config_doc["register_commands"]["local"]["enabled"].as_bool().expect("bad config spec") {
        config_doc["register_commands"]["local"]["guilds"].as_vec().iter().map(|id|
            GuildId::from(
                id[0].as_str().expect("bad config spec")
                    .parse::<u64>().expect("guild id not valid snowflake")
            )
        ).collect_vec()
    } else {
        vec![]
    };

    let framework = poise::Framework::<BotVars, BotError>::builder()
        .options(FrameworkOptions {
            commands: vec![ping()],
            prefix_options: PrefixFrameworkOptions {
                mention_as_prefix: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                if register_globally {
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    println!("registered commands globally");
                } else {
                    println!("not registering commands globally")
                }

                if !guilds_to_register_in.is_empty() {
                    for id in guilds_to_register_in.iter() {
                        poise::builtins::register_in_guild(ctx, &framework.options().commands, *id).await?;
                    }
                    println!("registered commands locally in {}", pluralize("guild", guilds_to_register_in.len() as isize, true));
                }
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
mod handler;
mod util;
mod commands;
mod model;

use crate::commands::{ewar, maint, meta};
use clap::ValueHint;
use itertools::Itertools;
use mongodb::bson::doc;
use pluralizer::pluralize;
use poise::{FrameworkOptions, PrefixFrameworkOptions};
use serenity::all::GatewayIntents;
use serenity::all::GuildId;
use serenity::Client;
use std::default::Default;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use tokio_postgres::NoTls;
use yaml_rust2::YamlLoader;

struct BotVars {
    mongo: mongodb::Database,
    postgres: deadpool_postgres::Pool,
    update_ratings_lock: async_std::sync::Arc<async_std::sync::Mutex<()>>
}

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

    let register_globally = config_doc["register_commands"]["global"].as_bool().expect("bad global register setting");
    let guilds_to_register_in = if config_doc["register_commands"]["local"]["enabled"].as_bool().expect("bad local register setting") {
        config_doc["register_commands"]["local"]["guilds"].as_vec().iter().map(|id|
            GuildId::from(
                id[0].as_str().expect("bad register guild id")
                    .parse::<u64>().expect("guild id not valid snowflake")
            )
        ).collect_vec()
    } else {
        vec![]
    };

    let mongo_uri = config_doc["creds"]["mongo"]["uri"].as_str().expect("bad mongo uri").to_string();
    let mongo_db = config_doc["creds"]["mongo"]["db"].as_str().expect("bad mongo db").to_string();

    let postgres_uri = config_doc["creds"]["postgres"].as_str().expect("bad postgres uri").to_string();

    let framework = poise::Framework::<BotVars, BotError>::builder()
        .options(FrameworkOptions {
            commands: vec![
                meta::ping(),
                meta::git(),
                maint::sql(),
                ewar::user::lookup(),
                ewar::user::register(),
                ewar::game::postgame(),
                ewar::game::review(),
                ewar::game::whatif_game(),
            ],
            prefix_options: PrefixFrameworkOptions {
                mention_as_prefix: true,
                ..Default::default()
            },
            initialize_owners: true,
            skip_checks_for_owners: true,
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                let commands_count = pluralize("command", framework.options().commands.len() as isize, true);

                if register_globally {
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    println!("registered {commands_count} globally");
                } else {
                    println!("not registering {commands_count} globally");
                }

                if !guilds_to_register_in.is_empty() {
                    for id in guilds_to_register_in.iter() {
                        poise::builtins::register_in_guild(ctx, &framework.options().commands, *id).await?;
                    }
                    println!("registered {commands_count} locally in {}", pluralize("guild", guilds_to_register_in.len() as isize, true));
                }

                let mongo = mongodb::Client::with_uri_str(mongo_uri).await?.database(&*mongo_db);
                mongo.run_command(doc! { "ping": 1 }).await?;
                println!("mongo ok");

                let pg_config = tokio_postgres::Config::from_str(&*postgres_uri)?;
                let mgr_config = deadpool_postgres::ManagerConfig {
                    recycling_method: deadpool_postgres::RecyclingMethod::Fast,
                };
                let mgr = deadpool_postgres::Manager::from_config(pg_config, NoTls, mgr_config);
                let pg_pool = deadpool_postgres::Pool::builder(mgr).max_size(16).build().unwrap();
                println!("postgres ok");

                Ok(BotVars {
                    mongo,
                    postgres: pg_pool,
                    update_ratings_lock: Default::default(),
                })
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
mod commands;
mod handler;
mod model;
mod util;

use crate::commands::{ewar, maint, meta};
use crate::model::StandingEventInner::InactivityDecay;
use crate::model::{ApprovalStatus, LeagueInfo, Player, StandingEvent};
use chrono::{TimeDelta, Utc};
use clap::ValueHint;
use futures::TryStreamExt;
use itertools::Itertools;
use mongodb::bson::doc;
use mongodb::Database;
use pluralizer::pluralize;
use poise::{FrameworkOptions, PrefixFrameworkOptions};
use serenity::all::GuildId;
use serenity::all::{GatewayIntents, UserId};
use serenity::Client;
use std::collections::HashSet;
use std::default::Default;
use std::env;
use std::path::PathBuf;
use tokio_cron::{daily, Job, Scheduler};

async fn inactivity_decay_job(mongo_uri: String, mongo_db: String) -> Result<(), BotError> {
    let mongo = mongodb::Client::with_uri_str(mongo_uri)
        .await?
        .database(&*mongo_db);

    inactivity_decay_inner(&mongo).await
}

async fn inactivity_decay_inner(mongo: &Database) -> Result<(), BotError> {
    let victims = mongo
        .collection::<Player>("players")
        .find(doc! {
            "last_played": {
                "$lt": bson::DateTime::from_chrono(Utc::now() - TimeDelta::days(7))
            }
        })
        .await?
        .try_filter_map(|p| async move { Ok(Some(p._id)) })
        .try_collect::<Vec<_>>()
        .await?;

    let LeagueInfo {
        available_event_number,
        ..
    } = mongo
        .collection::<LeagueInfo>("league_info")
        .find_one_and_update(doc! {}, doc! { "$inc": { "available_event_number": 1 } })
        .await?
        .expect("league_info struct missing");

    mongo
        .collection::<StandingEvent>("events")
        .insert_one(StandingEvent {
            _id: available_event_number,
            approval_status: Some(ApprovalStatus {
                approved: true,
                reviewer: None,
            }),
            inner: InactivityDecay {
                victims,
                delta_deviation: 0.1,
            },
            when: Utc::now(),
        })
        .await?;

    Ok(())
}

struct BotVars {
    mongo: Database,
    core_state_lock: async_std::sync::Arc<async_std::sync::Mutex<()>>,
    league_moderators: HashSet<UserId>,
}

#[tokio::main]
async fn main() {
    let cmd = clap::command!("ewar-bot")
        .about("Discord bot for handling ranked Egyptian War backed by TrueSkill")
        .arg(clap::arg!(<"config"> ".env file path")
            .value_parser(clap::value_parser!(PathBuf))
            .value_hint(ValueHint::FilePath)
            .required(false)
            .default_value(".env"));

    let args = cmd.get_matches();
    dotenv::from_filename(args.get_one::<PathBuf>("config").expect("config file is bad path?")).ok();

    let register_globally = env::var("EWAR_REGISTER_GLOBAL").is_ok();
    let guilds_to_register_in = match env::var("EWAR_REGISTER_LOCAL").is_ok() {
        true => env::var("EWAR_GUILDS")
            .unwrap_or(String::from(""))
            .split(",")
            .map(String::from)
            .map(|s| String::from(s.trim()))
            .map(|id| GuildId::from(id.parse::<u64>().expect("guild id not valid snowflake")))
            .collect_vec(),
        false => vec![],
    };

    let mongo_uri = env::var("EWAR_MONGO_URI").expect("bad mongo uri");
    let mongo_db = env::var("EWAR_MONGO_DB").expect("bad mongo db");

    let moderator_discord_ids = env::var("EWAR_LEAGUE_MODERATORS")
        .unwrap_or(String::from(""))
        .split(",")
        .map(String::from)
        // dumb allocation but meh
        .map(|s| String::from(s.trim()))
        .map(|id| {
            UserId::from(
                id.parse::<u64>()
                    .expect("league moderator discord id not valid snowflake"),
            )
        })
        .collect_vec();

    let mut scheduler = Scheduler::local();
    {
        let mongo_uri = mongo_uri.clone();
        let mongo_db = mongo_db.clone();
        scheduler.add(Job::named("inactivity_decay", daily("0"), move || {
            let mongo_uri = mongo_uri.clone();
            let mongo_db = mongo_db.clone();
            async move {
                match inactivity_decay_job(mongo_uri, mongo_db).await.err() {
                    None => {}
                    Some(err) => {
                        eprintln!("{}", err)
                    }
                }
            }
        }));
        println!("cron job for decay ok")
    }

    let framework = poise::Framework::<BotVars, BotError>::builder()
        .options(FrameworkOptions {
            commands: vec![
                meta::ping(),
                meta::git(),
                maint::advance_pointer(),
                maint::fsck(),
                maint::force_reprocess(),
                maint::do_decay(),
                maint::pop_event(),
                ewar::event::event(),
                ewar::user::user(),
                ewar::user::register(),
                ewar::game::game(),
                ewar::moderation::review(),
                ewar::moderation::unreviewed(),
                ewar::moderation::penalize(),
                ewar::moderation::force_register(),
                ewar::moderation::lb_blacklist(),
                ewar::leaderboard::leaderboard(),
            ],
            prefix_options: PrefixFrameworkOptions {
                mention_as_prefix: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                let commands_count =
                    pluralize("command", framework.options().commands.len() as isize, true);

                if register_globally {
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    println!("registered {commands_count} globally");
                } else {
                    println!("not registering {commands_count} globally");
                }

                if !guilds_to_register_in.is_empty() {
                    for id in guilds_to_register_in.iter() {
                        poise::builtins::register_in_guild(ctx, &framework.options().commands, *id)
                            .await?;
                    }
                    println!(
                        "registered {commands_count} locally in {}",
                        pluralize("guild", guilds_to_register_in.len() as isize, true)
                    );
                }

                let mongo = mongodb::Client::with_uri_str(mongo_uri)
                    .await?
                    .database(&*mongo_db);
                mongo.run_command(doc! { "ping": 1 }).await?;
                println!("mongo ok");

                Ok(BotVars {
                    mongo,
                    core_state_lock: Default::default(),
                    league_moderators: moderator_discord_ids.into_iter().collect(),
                })
            })
        })
        .build();

    let token = env::var("EWAR_DISCORD_TOKEN").expect("no discord token set");
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

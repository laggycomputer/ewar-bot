mod handler;
mod util;
mod commands;

use crate::commands::meta::ping;
use dotenv::dotenv;
use poise::FrameworkOptions;
use serenity::all::GatewayIntents;
use serenity::all::GuildId;
use serenity::Client;
use std::env;

struct BotVars {}

#[tokio::main]
async fn main() {
    dotenv().ok();

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
                Ok(BotVars {})
            })
        })
        .build();

    let token = env::var("token").expect("discord client token missing");
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
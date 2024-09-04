mod handler;
mod util;

use dotenv::dotenv;
use serenity::all::GatewayIntents;
use serenity::Client;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();

    let token = env::var("token").expect("discord client token missing");

    let mut client = Client::builder(&token, GatewayIntents::all())
        .event_handler(handler::EWarBotHandler)
        .await
        .expect("couldn't make client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}

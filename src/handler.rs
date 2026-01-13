use crate::util::bot_invite_url;
use core::time::Duration;
use rand::prelude::IndexedRandom as _;
use serenity::all::ActivityData;
use serenity::all::ActivityType;
use serenity::all::Context;
use serenity::all::EventHandler;
use serenity::all::OnlineStatus;
use serenity::all::Permissions;
use serenity::all::Ready;
use serenity::async_trait;
use tokio::time;

pub(crate) struct EWarBotHandler;

#[async_trait]
impl EventHandler for EWarBotHandler {
    async fn ready(&self, ctx: Context, ready_info: Ready) {
        println!(
            "ok, connected as {} (UID {})",
            ready_info.user.tag(),
            ready_info.user.id
        );
        println!("using discord API version {}", ready_info.version);
        println!(
            "invite link: {}",
            bot_invite_url(ready_info.user.id, Permissions::empty(), true)
        );

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(120));

            let status = [
                "playing egyptian war",
                "safety slapping",
                "gambling away cards",
                "counting until the jack comes back",
                "burning to a slap",
                "slapping too hard",
                "burning the wrong card",
                "losing rating",
            ];

            loop {
                ctx.shard.set_presence(
                    Some(ActivityData {
                        name: String::from("bazinga"),
                        kind: ActivityType::Custom,

                        state: Some(String::from(*status.choose(&mut rand::rng()).unwrap())),
                        url: None,
                    }),
                    OnlineStatus::Idle,
                );
                interval.tick().await;
            }
        });
        println!("status cycling active");
    }
}

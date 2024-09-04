use serenity::all::{Context, EventHandler, Message, Permissions, Ready};
use serenity::async_trait;
use crate::util::bot_invite_url;

pub(crate) struct EWarBotHandler;

#[async_trait]
impl EventHandler for EWarBotHandler {
    async fn message(&self, ctx: Context, msg: Message) {
        println!("{}", msg.content);
    }

    async fn ready(&self, ctx: Context, ready_info: Ready) {
        println!("ok, connected as {} (UID {})", ready_info.user.tag(), ready_info.user.id);
        println!("using discord API version {}", ready_info.version);
        println!("invite link: {}", bot_invite_url(ready_info.user.id, Permissions::empty(), true))
    }
}
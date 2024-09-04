use crate::{BotError, Context};
#[poise::command(slash_command, prefix_command)]
pub(crate) async fn ping(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.say("ok").await?;
    Ok(())
}
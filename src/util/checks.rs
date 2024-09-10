use crate::{BotError, Context};

pub(crate) async fn is_league_moderator(ctx: Context<'_>) -> Result<bool, BotError> {
    let cond = ctx.author().id.get() == 328678556899213322;

    if !cond {
        ctx.reply(":x: must be league moderator to do this").await?;
    }

    Ok(cond)
}
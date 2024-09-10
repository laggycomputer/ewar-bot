use crate::{BotError, Context};

pub(crate) async fn is_league_moderator(ctx: Context<'_>) -> Result<bool, BotError> {
    Ok(ctx.author().id.get() == 328678556899213322)
}
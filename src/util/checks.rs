use crate::{BotError, Context};

pub(crate) async fn league_moderators(ctx: Context<'_>) -> Result<bool, BotError> {
    Ok(ctx.author().id.get() == 328678556899213322)
}
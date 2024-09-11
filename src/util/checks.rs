use crate::commands::ewar::user::{try_lookup_user, UserLookupType};
use crate::{BotError, Context};
use poise::CreateReply;

pub(crate) async fn _is_league_moderator(ctx: Context<'_>) -> Result<bool, BotError> {
    Ok(ctx.data().league_moderators.contains(&ctx.author().id))
}

pub(crate) async fn is_league_moderator(ctx: Context<'_>) -> Result<bool, BotError> {
    let cond = _is_league_moderator(ctx).await?;

    if !cond {
        ctx.send(CreateReply::default()
            .content(":x: must be league moderator to do this")
            .ephemeral(true)).await?;
    }

    Ok(cond)
}

pub(crate) async fn has_system_account(ctx: Context<'_>) -> Result<bool, BotError> {
    let conn = ctx.data().postgres.get().await?;
    let cond = try_lookup_user(&conn, UserLookupType::DiscordID(ctx.author().id.get())).await?.is_some();

    if !cond {
        ctx.send(CreateReply::default()
            .content(":x: do you have an account on the system?")
            .ephemeral(true)).await?;
    }

    Ok(cond)
}
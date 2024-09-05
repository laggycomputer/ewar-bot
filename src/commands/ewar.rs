use crate::{BotError, Context};
use serenity::all::User;

/// Look up a user in the database
#[poise::command(slash_command, prefix_command, subcommands("user"))]
pub(crate) async fn lookup(ctx: Context<'_>) -> Result<(), BotError> {
    Ok(())
}

/// defaults to you; look up a player by discord user
#[poise::command(slash_command, prefix_command)]
async fn user(ctx: Context<'_>, user: Option<User>) -> Result<(), BotError> {
    let user = user.as_ref().unwrap_or(ctx.author());

    let conn = ctx.data().postgres.get().await?;

    let prepared = conn.prepare_typed(
        "SELECT * FROM players LEFT JOIN player_discord ON players.player_id = player_discord.player_id WHERE player_discord.discord_user_id = $1::BIGINT;",
        &[tokio_postgres::types::Type::INT8],
    ).await?;

    match conn.query_opt(&prepared, &[&(user.id.get() as i64)]).await? {
        None => ctx.reply("could not find that player").await?,
        Some(row) => todo!()
    };

    Ok(())
}

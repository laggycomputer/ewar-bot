use crate::{BotError, Context};
use serenity::all::User;

/// Look up a user in the database
#[poise::command(slash_command, prefix_command, subcommands("user"))]
pub(crate) async fn lookup(ctx: Context<'_>) -> Result<(), BotError> {
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
async fn user(ctx: Context<'_>, user: User) -> Result<(), BotError> {
    let prepared = ctx.data().postgres.prepare_typed(
        "SELECT * FROM players LEFT JOIN player_discord ON players.player_id = player_discord.player_id WHERE player_discord.discord_user_id = $1::BIGINT;",
        &[tokio_postgres::types::Type::INT8],
    ).await?;

    match ctx.data().postgres.query_opt(&prepared, &[&(ctx.author().id.get() as i64)]).await? {
        None => ctx.reply("could not find that discord user").await?,
        Some(row) => todo!()
    };

    Ok(())
}

use crate::util::{base_embed, remove_markdown};
use crate::{BotError, Context};
use itertools::Itertools;
use poise::CreateReply;
use regex::RegexBuilder;
use serenity::all::User;
use tokio_postgres::Row;


/// shared postlude to every lookup method; just show the user
async fn lookup_result(ctx: Context<'_>, rows: Vec<Row>) -> Result<(), BotError> {
    let mut assoc_accounts = rows.iter().map(|row| format!("<@{}>", row.get::<&str, i64>("discord_user_id"))).join(", ");
    if assoc_accounts.is_empty() {
        assoc_accounts = String::from("<none>")
    }

    ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .field("user", format!("{} (ID {})",
                                   remove_markdown(rows[0].get::<&str, String>("player_name")),
                                   rows[0].get::<&str, i32>("player_id")), true)
            .field("rating stuff", "todo", true)
            .description("associated discord accounts: ".to_owned() + &assoc_accounts))).await?;

    Ok(())
}

/// Look up a user in the database
#[poise::command(slash_command, prefix_command, subcommands("user", "name", "id"))]
pub(crate) async fn lookup(ctx: Context<'_>) -> Result<(), BotError> {
    Ok(())
}

/// defaults to you; look up a player by discord user
#[poise::command(slash_command, prefix_command)]
async fn user(ctx: Context<'_>, user: Option<User>) -> Result<(), BotError> {
    let user = user.as_ref().unwrap_or(ctx.author());

    let conn = ctx.data().postgres.get().await?;

    let rows = conn.query(
        "SELECT player_name, player_discord.player_id, discord_user_id FROM players LEFT JOIN player_discord \
        ON players.player_id = player_discord.player_id WHERE player_discord.discord_user_id = $1::BIGINT;",
        &[&(user.id.get() as i64)]).await?;
    if rows.is_empty() {
        ctx.reply("could not find player with that discord user").await?;
        return Ok(());
    }

    lookup_result(ctx, rows).await
}

/// look up a player by handle
#[poise::command(slash_command, prefix_command)]
async fn name(ctx: Context<'_>, handle: String) -> Result<(), BotError> {
    let conn = ctx.data().postgres.get().await?;

    let rows = conn.query(
        "SELECT player_name, player_discord.player_id, discord_user_id FROM players LEFT JOIN player_discord \
        ON players.player_id = player_discord.player_id WHERE player_name = $1;",
        &[&handle]).await?;
    if rows.is_empty() {
        ctx.reply("could not find player by that handle").await?;
        return Ok(());
    }

    lookup_result(ctx, rows).await
}

/// look up a player by database ID
#[poise::command(slash_command, prefix_command)]
async fn id(ctx: Context<'_>, id: i32) -> Result<(), BotError> {
    let conn = ctx.data().postgres.get().await?;

    let rows = conn.query(
        "SELECT player_name, player_discord.player_id, discord_user_id FROM players LEFT JOIN player_discord \
        ON players.player_id = player_discord.player_id WHERE players.player_id = $1;",
        &[&id]).await?;
    if rows.is_empty() {
        ctx.reply("could not find player by that ID").await?;
        return Ok(());
    }

    lookup_result(ctx, rows).await
}

#[poise::command(slash_command, prefix_command)]
pub(crate) async fn register(ctx: Context<'_>, desired_name: String) -> Result<(), BotError> {
    let mut conn = ctx.data().postgres.get().await?;

    match conn.query_opt(
        "SELECT player_discord.player_id, player_name FROM players LEFT JOIN player_discord \
        ON players.player_id = player_discord.player_id WHERE player_discord.discord_user_id = $1::BIGINT;",
        &[&(ctx.author().id.get() as i64)]).await? {
        Some(row) => {
            ctx.reply(format!(
                "cannot bind your discord to a second user (currently bound to user {}, ID {})",
                remove_markdown(row.get::<&str, String>("player_name")),
                row.get::<&str, i32>("player_id")
            )).await?;
        }
        None => {
            let valid_pattern = RegexBuilder::new(r"^[a-z\d_.]{1,32}$")
                .case_insensitive(true)
                .build()
                .unwrap();

            if desired_name.len() > 32 {
                ctx.reply("name too long, sorry").await?;
                return Ok(());
            } else if !valid_pattern.is_match(&*desired_name) {
                ctx.reply("only alphanumeric, `_`, or `.`, sorry").await?;
                return Ok(());
            }

            let trans = conn.build_transaction()
                .deferrable(true)
                .start().await?;

            let new_id: i32 = trans.query_one(
                "INSERT INTO players (player_name) VALUES ($1) RETURNING player_id;",
                &[&desired_name],
            ).await?
                .get("player_id");
            trans.execute(
                "INSERT INTO player_discord (player_id, discord_user_id) VALUES ($1, $2);",
                &[&new_id, &(ctx.author().id.get() as i64)],
            ).await?;
            trans.commit().await?;

            ctx.reply(format!("welcome new user {}, ID {}", remove_markdown(desired_name), new_id)).await?;
        }
    };

    Ok(())
}

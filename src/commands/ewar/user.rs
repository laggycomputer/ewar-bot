use crate::model::PlayerID;
use crate::util::{base_embed, remove_markdown};
use crate::{BotError, Context};
use itertools::Itertools;
use poise::CreateReply;
use regex::RegexBuilder;
use serenity::all::{Mentionable, User, UserId};

enum UserLookupType<'a> {
    DiscordID(u64),
    SystemHandle(&'a str),
    SystemID(PlayerID),
}

async fn try_lookup_user(pg_conn: &deadpool_postgres::Object, how: UserLookupType<'_>)
                         -> Result<Option<(PlayerID, Box<str>, Vec<UserId>)>, BotError> {
    match match how {
        // try to get a row
        UserLookupType::DiscordID(id) => pg_conn.query_opt(
            "SELECT player_id FROM player_discord WHERE discord_user_id = $1::BIGINT;",
            &[&(id as i64)]).await?,
        UserLookupType::SystemHandle(handle) => pg_conn.query_opt(
            "SELECT player_id FROM players WHERE player_name = $1;",
            &[&handle]).await?,
        UserLookupType::SystemID(id) => pg_conn.query_opt(
            "SELECT player_id FROM players WHERE player_id = $1;",
            &[&id]).await?,
    } {
        // check if it's there
        None => Ok(None),
        Some(row) => {
            let player_id = row.get::<&str, PlayerID>("player_id");
            Ok(Some((
                player_id,
                pg_conn.query_one("SELECT player_name FROM players WHERE player_id = $1;", &[&player_id]).await?
                    .get("player_name"),
                pg_conn.query("SELECT discord_user_id FROM player_discord WHERE player_id = $1;", &[&player_id]).await?
                    .into_iter().map(|row| (row.get::<&str, i64>("discord_user_id") as u64).into())
                    .collect_vec()
            )))
        }
    }
}

/// shared postlude to every lookup method; just show the user
async fn display_lookup_result(ctx: Context<'_>, looked_up: (PlayerID, Box<str>, Vec<UserId>)) -> Result<(), BotError> {
    let (system_id, system_handle, discord_ids) = looked_up;

    let mut assoc_accounts = discord_ids.iter().map(UserId::mention).join(", ");
    if assoc_accounts.is_empty() {
        assoc_accounts = String::from("<none>")
    }

    ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .field("user", format!("{} (ID {})",
                                   remove_markdown(&*system_handle),
                                   system_id), true)
            .field("rating stuff", "todo", true)
            .description("associated discord accounts: ".to_owned() + &assoc_accounts))).await?;

    Ok(())
}

/// Look up a user in the database
#[poise::command(slash_command, prefix_command, subcommands("user", "name", "id"))]
pub(crate) async fn lookup(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.reply("base command is noop, try a subcommand").await?;

    Ok(())
}

/// defaults to you; look up a player by discord user
#[poise::command(slash_command, prefix_command)]
async fn user(ctx: Context<'_>, #[description = "Discord user to lookup by"] user: Option<User>) -> Result<(), BotError> {
    let user = user.as_ref().unwrap_or(ctx.author());

    match try_lookup_user(&ctx.data().postgres.get().await?, UserLookupType::DiscordID(user.id.into())).await? {
        None => {
            ctx.reply("could not find player with that discord user").await?;
        }
        Some(looked_up) => {
            display_lookup_result(ctx, looked_up).await?
        }
    }

    Ok(())
}

/// look up a player by handle
#[poise::command(slash_command, prefix_command)]
async fn name(ctx: Context<'_>, #[description = "System handle to lookup by"] handle: String) -> Result<(), BotError> {
    match try_lookup_user(&ctx.data().postgres.get().await?, UserLookupType::SystemHandle(handle.as_str())).await? {
        None => {
            ctx.reply("could not find player by that handle").await?;
        }
        Some(looked_up) => {
            display_lookup_result(ctx, looked_up).await?
        }
    }

    Ok(())
}

/// look up a player by database ID
#[poise::command(slash_command, prefix_command)]
async fn id(ctx: Context<'_>, #[description = "System ID to lookup by"] id: PlayerID) -> Result<(), BotError> {
    match try_lookup_user(&ctx.data().postgres.get().await?, UserLookupType::SystemID(id)).await? {
        None => {
            ctx.reply("could not find player by that ID").await?;
        }
        Some(looked_up) => {
            display_lookup_result(ctx, looked_up).await?
        }
    }

    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub(crate) async fn register(ctx: Context<'_>, #[description = "Defaults to your Discord username - name you want upon registration"] desired_name: Option<String>) -> Result<(), BotError> {
    let proposed_name = desired_name.unwrap_or(ctx.author().name.clone());

    let mut pg_conn = ctx.data().postgres.get().await?;

    match pg_conn.query_opt(
        "SELECT player_discord.player_id, player_name FROM players LEFT JOIN player_discord \
        ON players.player_id = player_discord.player_id WHERE player_discord.discord_user_id = $1::BIGINT;",
        &[&(ctx.author().id.get() as i64)]).await? {
        Some(row) => {
            ctx.reply(format!(
                "cannot bind your discord to a second user (currently bound to user {}, ID {})",
                remove_markdown(row.get("player_name")),
                row.get::<&str, i32>("player_id")
            )).await?;
        }
        None => {
            if pg_conn.query_opt("SELECT 1 FROM players WHERE player_name = $1;", &[&proposed_name.as_str()]).await?.is_some() {
                ctx.reply(format!("user by name {proposed_name} already exists")).await?;
                return Ok(());
            }

            let valid_pattern = RegexBuilder::new(r"^[a-z\d_.]{1,32}$")
                .case_insensitive(true)
                .build().unwrap();

            if proposed_name.len() > 32 {
                ctx.reply("name too long, sorry").await?;
                return Ok(());
            } else if !valid_pattern.is_match(&*proposed_name) {
                ctx.reply("only alphanumeric, `_`, or `.`, sorry").await?;
                return Ok(());
            }

            let trans = pg_conn.build_transaction()
                .deferrable(true)
                .start().await?;

            let new_id: i32 = trans.query_one(
                "INSERT INTO players (player_name) VALUES ($1) RETURNING player_id;",
                &[&proposed_name],
            ).await?
                .get("player_id");
            trans.execute(
                "INSERT INTO player_discord (player_id, discord_user_id) VALUES ($1, $2);",
                &[&new_id, &(ctx.author().id.get() as i64)],
            ).await?;
            trans.commit().await?;

            ctx.reply(format!("welcome new user {}, ID {}", remove_markdown(&*proposed_name), new_id)).await?;
        }
    };

    Ok(())
}

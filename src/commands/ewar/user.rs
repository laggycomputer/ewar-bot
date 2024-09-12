use crate::commands::ewar::user::UserLookupType::{DiscordID, Username};
use crate::model::StandingEventInner::{InactivityDecay, JoinLeague};
use crate::model::{ApprovalStatus, LeagueInfo, Player, PlayerID, StandingEvent};
use crate::util::constants::DEFAULT_RATING;
use crate::util::rating::RatingExtra;
use crate::util::{base_embed, remove_markdown};
use crate::{BotError, Context};
use bson::doc;
use chrono::Utc;
use futures::TryStreamExt;
use itertools::Itertools;
use mongodb::Database;
use poise::CreateReply;
use regex::RegexBuilder;
use serenity::all::{Mentionable, User, UserId};
use skillratings::trueskill::TrueSkillRating;
use timeago::TimeUnit::Minutes;

pub(crate) enum UserLookupType<'a> {
    DiscordID(u64),
    Username(&'a str),
    SystemID(PlayerID),
}

pub(crate) async fn try_lookup_player(mongo: &Database, how: UserLookupType<'_>) -> Result<Option<Player>, BotError> {
    Ok(mongo.collection::<Player>("players").find_one(match how {
        UserLookupType::DiscordID(id) => doc! { "discord_ids": id as i64 },
        UserLookupType::Username(handle) => doc! { "username": handle },
        UserLookupType::SystemID(id) => doc! { "_id": id },
    }).await?)
}

/// shared postlude to every lookup method; just show the user
async fn display_lookup_result(ctx: Context<'_>, looked_up: Player) -> Result<(), BotError> {
    let events = ctx.data().mongo.collection::<StandingEvent>("events")
        .find(doc! {
            "$or": [
                { "inner.Penalty.victims": looked_up._id },
                { "inner.InactivityDecay.victims": looked_up._id },
                { "inner.JoinLeague.victims": looked_up._id },
                { "inner.GameEnd.ranking": looked_up._id },
            ]
        }).sort(doc! {"_id": -1})
        .limit(10).await?
        .try_collect::<Vec<_>>().await?;

    let mut event_lines = Vec::with_capacity(events.len());
    let mut consec_decay = 0;
    for event in events {
        match &event.inner {
            InactivityDecay { .. } => consec_decay += 1,
            _ => {
                match consec_decay {
                    0 => {}
                    1 => event_lines.push("<inactivity decay>".to_string().into_boxed_str()),
                    n => event_lines.push(format!("<inactivity decay> x{n}").into_boxed_str())
                }
                consec_decay = 0;
                event_lines.push(event.short_summary(&ctx.data().mongo).await?);
            }
        }
    }

    let mut assoc_accounts = looked_up.discord_ids.iter()
        .map(|id| UserId::try_from(*id).unwrap().mention())
        .join(", ");
    if assoc_accounts.is_empty() {
        assoc_accounts = String::from("<none>")
    }

    let mut time_formatter = timeago::Formatter::new();
    time_formatter
        .num_items(2)
        .min_unit(Minutes);

    let rating = looked_up.rating_struct();

    ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .field("user",
                   format!("{} (ID {})",
                           remove_markdown(&*looked_up.username),
                           looked_up._id), true)
            .field("rating stuff", format!(
                "{} (true rating {:.2}, deviation {:.2}){}",
                rating.format_rating(),
                rating.rating,
                rating.uncertainty,
                if rating.is_provisional() { "; __this rating is provisional until deviation falls under 2.5__" } else { "" }
            ), true)
            .field("last played", looked_up.last_played
                .map(|dt| format!("<t:{}:f> ({})", dt.timestamp(), time_formatter.convert_chrono(dt, Utc::now())))
                .unwrap_or("never".to_string()),
                   true)
            .field("associated discord accounts", assoc_accounts, true)
            .description(format!("recent events:\n\n{}", event_lines.into_iter().join("\n"))))).await?;
    Ok(())
}

/// Look up a user in the database
#[poise::command(prefix_command, slash_command, subcommands("by_discord", "by_username", "by_id"))]
pub(crate) async fn user(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.reply("base command is noop, try a subcommand").await?;

    Ok(())
}

/// defaults to you; look up a player by discord user
#[poise::command(prefix_command, slash_command)]
async fn by_discord(ctx: Context<'_>, #[description = "Discord user to lookup by"] user: Option<User>) -> Result<(), BotError> {
    let user = user.as_ref().unwrap_or(ctx.author());

    match try_lookup_player(&ctx.data().mongo, UserLookupType::DiscordID(user.id.into())).await? {
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
#[poise::command(prefix_command, slash_command)]
async fn by_username(ctx: Context<'_>, #[description = "System handle to lookup by"] handle: String) -> Result<(), BotError> {
    match try_lookup_player(&ctx.data().mongo, Username(handle.as_str())).await? {
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
#[poise::command(prefix_command, slash_command)]
async fn by_id(ctx: Context<'_>, #[description = "System ID to lookup by"] id: PlayerID) -> Result<(), BotError> {
    match try_lookup_player(&ctx.data().mongo, UserLookupType::SystemID(id)).await? {
        None => {
            ctx.reply("could not find player by that ID").await?;
        }
        Some(looked_up) => {
            display_lookup_result(ctx, looked_up).await?
        }
    }

    Ok(())
}

#[poise::command(prefix_command, slash_command)]
pub(crate) async fn register(ctx: Context<'_>, #[description = "Defaults to your Discord username - name you want upon registration"] desired_name: Option<String>) -> Result<(), BotError> {
    let proposed_name = desired_name.unwrap_or(ctx.author().name.clone()).to_lowercase();

    match try_lookup_player(&ctx.data().mongo, DiscordID(ctx.author().id.get())).await? {
        Some(player) => {
            ctx.reply(format!("cannot bind your discord to a second user (currently bound to user {})",
                              player.reference_no_discord()))
                .await?;
            return Ok(());
        }
        None => {}
    };

    if try_lookup_player(&ctx.data().mongo, Username(&*proposed_name)).await?.is_some() {
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

    let TrueSkillRating { rating, uncertainty, .. } = DEFAULT_RATING;

    let LeagueInfo { available_event_number, available_player_id, .. } = ctx.data().mongo
        .collection::<LeagueInfo>("league_info")
        .find_one_and_update(
            doc! {},
            doc! { "$inc": { "available_event_number": 1, "available_player_id": 1, } })
        .await?
        .expect("league_info struct missing");

    // add player
    let new_player = Player {
        _id: available_player_id,
        username: proposed_name,
        rating,
        deviation: uncertainty,
        last_played: None,
        discord_ids: vec![ctx.author().id.get()],
    };
    ctx.data().mongo.collection::<Player>("events").insert_one(&new_player).await?;

    // add league join event
    ctx.data().mongo.collection::<StandingEvent>("events").insert_one(StandingEvent {
        _id: available_event_number,
        approval_status: Some(ApprovalStatus {
            approved: true,
            reviewer: None,
        }),
        inner: JoinLeague {
            victims: vec![available_player_id],
            initial_rating: rating,
            initial_deviation: uncertainty,
        },
        when: Utc::now(),
    }).await?;

    ctx.reply(format!("welcome new user {}", new_player.reference_no_discord())).await?;

    Ok(())
}

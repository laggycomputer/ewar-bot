use crate::commands::ewar::user::try_lookup_user;
use crate::commands::ewar::user::UserLookupType::SystemID;
use crate::ewar::game::BadPlacementType::*;
use crate::ewar::user::UserLookupType;
use crate::model::ApprovalStatus;
use crate::model::StandingEventInner::GameEnd;
use crate::model::{Game, GameID, LeagueInfo, PlayerID, StandingEvent};
use crate::util::base_embed;
use crate::util::checks::{_is_league_moderator, has_system_account};
use crate::util::rating::game_affect_ratings;
use crate::util::rating::RatingExtra;
use crate::util::short_user_reference;
use crate::{BotError, Context};
use bson::doc;
use chrono::{TimeDelta, Utc};
use itertools::Itertools;
use poise::CreateReply;
use serenity::all::{CreateActionRow, CreateButton, CreateInteractionResponse, CreateInteractionResponseMessage, EditMessage, Mentionable, ReactionType, User};
use skillratings::trueskill::TrueSkillRating;
use std::collections::HashSet;
use std::convert::identity;
use std::time::Duration;
use timeago::TimeUnit::Seconds;

enum BadPlacementType {
    DuplicateUser,
    UserNotFound { offending: User },
}

impl BadPlacementType {
    fn create_error_message(&self, ctx: Context<'_>) -> CreateReply {
        match self {
            DuplicateUser => {
                CreateReply::default().content(":x: same user given twice; each player has exactly one ranking!")
            }
            UserNotFound { offending: user } => {
                CreateReply::default()
                    .embed(base_embed(ctx)
                        .description(format!("{} has no account on this bot", user.mention())))
            }
        }
    }
}

/// placements as discord user to (system username, system ID) pair
async fn placement_discord_to_system(placement: &Vec<User>, pg_conn: &deadpool_postgres::Object) -> Result<Result<Vec<(String, PlayerID, TrueSkillRating)>, BadPlacementType>, BotError> {
    if placement.len() != placement.iter().map(|u| u.id).collect::<HashSet<_>>().len() {
        return Ok(Err(DuplicateUser));
    }

    let mut placement_system_users: Vec<(String, PlayerID, TrueSkillRating)> = Vec::with_capacity(placement.len());
    for user in placement.clone().into_iter() {
        match pg_conn.query_opt("SELECT player_name, player_discord.player_id, rating, deviation \
        FROM players LEFT JOIN player_discord ON players.player_id = player_discord.player_id \
        WHERE player_discord.discord_user_id = $1::BIGINT;", &[&(user.id.get() as i64)]).await? {
            None => {
                return Ok(Err(UserNotFound { offending: user }))
            }
            Some(row) => {
                placement_system_users.push((
                    row.get("player_name"),
                    row.get("player_id"),
                    TrueSkillRating::from_row(&row),
                ));
            }
        }
    };

    Ok(Ok(placement_system_users))
}

#[poise::command(prefix_command, slash_command, subcommands("post", "whatif", "query"))]
pub(crate) async fn game(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.reply("base command is noop, try a subcommand").await?;

    Ok(())
}
/// Log a completed game with placement
#[poise::command(prefix_command, slash_command, check = has_system_account)]
pub(crate) async fn post(
    ctx: Context<'_>,
    #[description = "Time given for the game before overtime"] game_time: String,
    // AAAAAAAAA
    #[description = "The winner of the game"] user1: User,
    #[description = "#2 in the game"] user2: User,
    #[description = "#3, if applicable"] user3: Option<User>,
    #[description = "#4, if applicable"] user4: Option<User>,
    #[description = "#5, if applicable"] user5: Option<User>,
    #[description = "#6, if applicable"] user6: Option<User>,
    #[description = "#7, if applicable"] user7: Option<User>,
    #[description = "#8, if applicable"] user8: Option<User>,
    #[description = "#9, if applicable"] user9: Option<User>,
    #[description = "#10, if applicable"] user10: Option<User>,
    #[description = "#11, if applicable"] user11: Option<User>,
) -> Result<(), BotError> {
    // accept hh:mm:ss or mm:ss or ss
    let game_time = game_time.split(":").collect_vec();
    if game_time.len() > 3 || game_time.iter().any(|sec| sec.is_empty()) {
        ctx.send(CreateReply::default()
            .content(":x: bad format; hh:mm:ss or mm:ss or ss")
            .ephemeral(true)).await?;
        return Ok(());
    }
    let parts = game_time.into_iter().map(|sec| sec.parse::<u32>().ok()).rev().collect_vec();
    if parts.iter().any(|sec| sec.is_none()) {
        ctx.send(CreateReply::default()
            .content(":x: some part of your time was not a number")
            .ephemeral(true)).await?;
        return Ok(());
    }
    let unwrapped_parts = parts.into_iter().map(Option::unwrap).collect_vec();
    let time_seconds = unwrapped_parts.get(0).unwrap_or(&0)
        + 60 * unwrapped_parts.get(1).unwrap_or(&0)
        + 60 * 60 * unwrapped_parts.get(2).unwrap_or(&0);

    let submitted_time = Utc::now();

    let placement_discord = vec![
        Some(user1), Some(user2), user3, user4, user5, user6,
        user7, user8, user9, user10, user11,
    ].into_iter().filter_map(identity).collect_vec();

    // part 1: validate proposed game
    let pg_conn = ctx.data().postgres.get().await?;
    let poster_info = try_lookup_user(&pg_conn, UserLookupType::DiscordID(ctx.author().id.get())).await?
        .expect("user disappeared after check");

    let poster_not_moderator = !_is_league_moderator(ctx).await?;
    if poster_not_moderator && placement_discord.iter().all(|u| u != ctx.author()) {
        ctx.reply(":x: you must be a party to a game to log it").await?;
        return Ok(());
    }

    let placement_system_users = match placement_discord_to_system(&placement_discord, &pg_conn).await? {
        Err(reason) => {
            ctx.send(reason.create_error_message(ctx)).await?;
            return Ok(());
        }
        Ok(ret) => ret
    };

    // part 2: submitter must confirm
    let emb_desc = format!(
        "you are logging a game with the following result:\n{}\n{}",
        placement_discord.iter().zip(placement_system_users.iter()).enumerate()
            .map(|(index, (discord_user, (handle, id, _)))| format!(
                "{}. {} ({})", index + 1, discord_user.mention(), short_user_reference(handle, *id))
            )
            .join("\n"),
        if !poster_not_moderator {
            "\n**as a moderator, your confirmation will submit and approve the game immediately**"
        } else { "" });

    let initial_confirm_button = CreateButton::new("postgame_confirm_initial").emoji(ReactionType::Unicode(String::from("✅")));
    let reply = CreateReply::default()
        .embed(base_embed(ctx)
            .description(emb_desc.clone() + "\nplease click below if this is what you meant (10s timeout)"))
        .components(vec![
            CreateActionRow::Buttons(vec![
                initial_confirm_button.clone()])]);
    let msg_part2 = ctx.send(reply.clone()).await?;

    let waited = msg_part2.message().await?.await_component_interaction(&ctx.serenity_context().shard)
        .author_id(ctx.author().id)
        .custom_ids(vec![String::from("postgame_confirm_initial")])
        .timeout(Duration::from_secs(10)).await;

    if waited.is_none() {
        ctx.reply("timed out, submission of this game cancelled").await?;
        return Ok(());
    }

    let mut not_signed_off = placement_discord.clone().into_iter().collect::<HashSet<_>>();
    not_signed_off.remove(&ctx.author());

    // remove "please react below..." and button
    waited.unwrap().create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .embed(base_embed(ctx)
                .description(emb_desc))
            .components(vec![])
    )).await?;

    // part 3: parties to game must sign
    // moderators can skip this
    if poster_not_moderator {
        let make_signoff_msg = |not_signed_off: &HashSet<User>, disable_button: bool| (
            format!(
                "please sign off on this game with :white_check_mark:\n\
            simple majority is required to submit game\n\
            {}\n\
            \n\
            ~~struck through~~ players have already signed\n\
            **after 5 minutes of inactivity, game is rejected for submission**",
                placement_discord.iter().map(|user| {
                    if not_signed_off.contains(user) { user.mention().to_string() } else { format!("~~{}~~", user.mention()) }
                }).join("\n")),
            vec![
                CreateActionRow::Buttons(vec![
                    CreateButton::new("postgame_party_sign")
                        .emoji(ReactionType::Unicode(String::from("✅")))
                        .disabled(disable_button)])]);

        let (signoff_content, signoff_components) = make_signoff_msg(&not_signed_off, false);
        let mut party_sign_stage_msg = ctx.send(CreateReply::default()
            .content(signoff_content)
            .components(signoff_components)).await?
            .into_message().await?;

        while not_signed_off.len() >= ((placement_discord.len() / 2) as f32).ceil() as usize {
            let not_signed_off_freeze = not_signed_off.clone();
            match party_sign_stage_msg.await_component_interaction(&ctx.serenity_context().shard)
                .filter(move |ixn| {
                    not_signed_off_freeze.contains(&ixn.user)
                })
                .timeout(Duration::from_secs(5 * 60))
                .await {
                None => {
                    let (_, signoff_components) = make_signoff_msg(&not_signed_off, true);

                    party_sign_stage_msg.edit(
                        ctx.http(),
                        EditMessage::new()
                            .components(signoff_components)).await?;

                    party_sign_stage_msg.reply(ctx.http(), "timed out, this game is voided for submission").await?;

                    return Ok(());
                }
                Some(ixn) => {
                    ixn.create_response(ctx.http(), CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                        .content("ok, signed off on this game")
                        .ephemeral(true))).await?;

                    not_signed_off.remove(&ixn.user);

                    let (signoff_content, signoff_components) = make_signoff_msg(&not_signed_off, false);
                    party_sign_stage_msg.edit(
                        ctx.http(),
                        EditMessage::new()
                            .content(signoff_content)
                            .components(signoff_components))
                        .await?;
                }
            }
        }

        let (_, signoff_components) = make_signoff_msg(&not_signed_off, true);
        party_sign_stage_msg.edit(
            ctx.http(),
            EditMessage::new()
                .components(signoff_components)).await?;
    }

    // part 4: log it

    // increment, but the previous value is what we'll use
    // big idea is to prevent someone else from messing with us, so reserve then use
    let LeagueInfo { available_game_id, available_event_number, .. } = ctx.data().mongo
        .collection::<LeagueInfo>("league_info")
        .find_one_and_update(
            doc! {},
            doc! { "$inc": doc! { "available_game_id": 1, "available_event_number": 1, } })
        .await?
        .expect("league_info struct missing");

    let participant_system_ids = placement_system_users.iter().map(|(_, player_id, _)| *player_id).collect_vec();

    let signed_game = Game {
        game_id: available_game_id,
        ranking: participant_system_ids.clone(),
        length: time_seconds,
    };

    let event = StandingEvent {
        _id: available_event_number,
        approval_status: if poster_not_moderator { None } else {
            Some(ApprovalStatus {
                approved: true,
                reviewer: Some(poster_info.player_id),
            })
        },
        inner: GameEnd(signed_game),
        when: submitted_time,
    };

    ctx.data().mongo.collection::<StandingEvent>("events").insert_one(event).await?;

    // part 5: moderator must sign later
    ctx.send(CreateReply::default().content(
        if poster_not_moderator {
            format!(
                "ok, game with ID {available_game_id} submitted for moderator verification\n\
                **any moderator, please approve or reject this game with `/review {available_game_id}`.**",
            )
        } else {
            // if poster was a moderator, it has already been approved
            format!("ok, game with ID {available_game_id} recorded as event {available_event_number} bypassing player signoff")
        })).await?;

    Ok(())
}

/// See the results of a potential match
#[poise::command(prefix_command, slash_command)]
pub(crate) async fn whatif(
    ctx: Context<'_>,
    // AAAAAAAAA
    #[description = "The winner of the hypothetical game"] user1: User,
    #[description = "#2 in the hypothetical game"] user2: User,
    #[description = "#3, if applicable"] user3: Option<User>,
    #[description = "#4, if applicable"] user4: Option<User>,
    #[description = "#5, if applicable"] user5: Option<User>,
    #[description = "#6, if applicable"] user6: Option<User>,
    #[description = "#7, if applicable"] user7: Option<User>,
    #[description = "#8, if applicable"] user8: Option<User>,
    #[description = "#9, if applicable"] user9: Option<User>,
    #[description = "#10, if applicable"] user10: Option<User>,
    #[description = "#11, if applicable"] user11: Option<User>,
) -> Result<(), BotError> {
    let placement_discord = vec![
        Some(user1), Some(user2), user3, user4, user5, user6,
        user7, user8, user9, user10, user11,
    ].into_iter().filter_map(identity).collect_vec();

    let conn = ctx.data().postgres.get().await?;
    let placement_system_users = match placement_discord_to_system(&placement_discord, &conn).await? {
        Err(reason) => {
            ctx.send(reason.create_error_message(ctx)).await?;
            return Ok(());
        }
        Ok(ret) => ret
    };

    let new_ratings = game_affect_ratings(&placement_system_users.iter()
        .map(|(_, _, rating)| *rating)
        .collect_vec());

    let mut rating_supply_delta = 0f64;

    let mut leaderboard = String::new();
    for index in 0..placement_discord.len() {
        let old_rating = placement_system_users[index].2;
        let new_rating = new_ratings[index];
        let leaderboard_delta = new_rating.leaderboard_rating() - old_rating.leaderboard_rating();

        rating_supply_delta += new_rating.rating - old_rating.rating;

        leaderboard += &*(format!(
            "{}. {} → {} ({:+.2}): {} ({})\n",
            index + 1,
            old_rating.format_rating(),
            new_rating.format_rating(),
            leaderboard_delta,
            placement_discord[index].mention(),
            short_user_reference(&placement_system_users[index].0, placement_system_users[index].1),
        ))
    }

    leaderboard += &*format!("\n{:+.2} to true rating supply\n", rating_supply_delta);

    ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .description(leaderboard))).await?;

    Ok(())
}

/// Get a game by ID
#[poise::command(prefix_command, slash_command)]
pub(crate) async fn query(ctx: Context<'_>, #[description = "ID of game to get"] game_id: GameID) -> Result<(), BotError> {
    let pg_conn = ctx.data().postgres.get().await?;

    let event = match ctx.data().mongo
        .collection::<StandingEvent>("events")
        .find_one(doc! { "inner.GameEnd.game_id" : game_id}).await? {
        None => {
            ctx.reply("can't find that game").await?;
            return Ok(());
        }
        Some(event) => event
    };

    let StandingEvent { inner: GameEnd(game), .. } = event else { return Err("game-looking struct is not a game".into()) };

    let mut users_info = Vec::with_capacity(game.ranking.len());
    for player_id in game.ranking {
        users_info.push(try_lookup_user(&pg_conn, SystemID(player_id)).await?.expect("user in game DNE"));
    }

    let mut time_formatter = timeago::Formatter::new();
    time_formatter
        .num_items(2)
        .min_unit(Seconds);

    let ranking = users_info.into_iter().enumerate()
        .map(|(index, user)| format!("{}. {}", index + 1, user.short_summary()))
        .join("\n");

    let chrono_game_length = TimeDelta::from_std(Duration::from_secs(game.length as u64))?;

    ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .field("id", format!("game ID {}, event ID {}", game.game_id, event._id), true)
            .field("when", format!(
                "<t:{}:d> ({})",
                event.when.timestamp(),
                time_formatter.convert_chrono(event.when, Utc::now())
            ), true)
            .field("reviewer", match event.approval_status {
                None => String::from("not approved yet"),
                Some(approval_status) => String::from(approval_status.short_summary(&pg_conn).await?),
            }, true)
            .field("length (pre-overtime)", format!(
                "{:02}:{:02}", chrono_game_length.num_minutes(), chrono_game_length.num_seconds() % 60
            ), true)
            .description(ranking)
        )
        .reply(true)).await?;

    Ok(())
}

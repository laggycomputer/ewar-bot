use crate::model::{Game, GameID};
use crate::model::{LeagueInfo, PlayerID};
use crate::util::checks::league_moderators;
use crate::util::{base_embed, remove_markdown};
use crate::{BotError, Context};
use bson::doc;
use chrono::Utc;
use itertools::Itertools;
use poise::CreateReply;
use regex::RegexBuilder;
use serenity::all::{CreateActionRow, CreateButton, CreateInteractionResponse, CreateInteractionResponseMessage, EditMessage, Mentionable, ReactionType, User};
use std::collections::HashSet;
use std::convert::identity;
use std::time::Duration;
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
    ctx.reply("base command is noop, try a subcommand").await?;

    Ok(())
}

/// defaults to you; look up a player by discord user
#[poise::command(slash_command, prefix_command)]
async fn user(ctx: Context<'_>, #[description = "Discord user to lookup by"] user: Option<User>) -> Result<(), BotError> {
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
async fn name(ctx: Context<'_>, #[description = "System handle to lookup by"] handle: String) -> Result<(), BotError> {
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
async fn id(ctx: Context<'_>, #[description = "System ID to lookup by"] id: i32) -> Result<(), BotError> {
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
pub(crate) async fn register(ctx: Context<'_>, #[description = "Username you want upon registration"] desired_name: String) -> Result<(), BotError> {
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
            if conn.query_opt("SELECT 1 FROM players WHERE player_name = $1;", &[&desired_name.as_str()]).await?.is_some() {
                ctx.reply("user by that name already exists").await?;
                return Ok(());
            }

            let valid_pattern = RegexBuilder::new(r"^[a-z\d_.]{1,32}$")
                .case_insensitive(true)
                .build().unwrap();

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

/// Log a completed game with placement
#[poise::command(prefix_command, slash_command)]
pub(crate) async fn postgame(
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

    let placement = vec![
        Some(user1), Some(user2), user3, user4, user5, user6,
        user7, user8, user9, user10, user11,
    ].into_iter().filter_map(identity).collect_vec();

    // part 1: validate proposed game
    if placement.iter().all(|u| u != ctx.author()) {
        ctx.reply(":x: you must be a party to a game to log it").await?;
        return Ok(());
    }

    if placement.len() != placement.iter().map(|u| u.id).collect::<HashSet<_>>().len() {
        ctx.reply(":x: same user given twice; each player has exactly one ranking!").await?;
        return Ok(());
    }

    let conn = ctx.data().postgres.get().await?;
    let mut participants_friendly: Vec<(User, String, PlayerID)> = Vec::with_capacity(placement.len());
    for user in placement.clone().into_iter() {
        match conn.query_opt("SELECT player_name, player_discord.player_id, discord_user_id FROM players LEFT JOIN player_discord \
        ON players.player_id = player_discord.player_id WHERE player_discord.discord_user_id = $1::BIGINT;", &[&(user.id.get() as i64)]).await? {
            None => {
                ctx.send(CreateReply::default()
                    .embed(base_embed(ctx)
                        .description(format!("{} has no account on this bot", user.mention())))).await?;
                return Ok(());
            }
            Some(row) => {
                participants_friendly.push((user, row.get("player_name"), row.get("player_id")));
            }
        };
    }

    // part 2: submitter must confirm
    let emb_desc = format!(
        "you are logging a game with the following result:\n{}\n",
        participants_friendly.iter().enumerate()
            .map(|(index, (discord_user, handle, id))| format!("{}. {} ({}, ID {})", index + 1, discord_user.mention(), handle, id))
            .join("\n"));

    let initial_confirm_button = CreateButton::new("postgame_confirm_initial").emoji(ReactionType::Unicode(String::from("✅")));
    let reply = CreateReply::default()
        .embed(base_embed(ctx)
            .description(emb_desc.clone() + "\nplease click below if this is what you meant (10s timeout)"))
        .components(vec![
            CreateActionRow::Buttons(vec![
                initial_confirm_button.clone()])]);
    let msg = ctx.send(reply.clone()).await?;

    let waited = msg.into_message().await?.await_component_interaction(&ctx.serenity_context().shard)
        .author_id(ctx.author().id)
        .custom_ids(vec![String::from("postgame_confirm_initial")])
        .timeout(Duration::from_secs(10)).await;

    if waited.is_none() {
        return Ok(());
    }

    let mut not_signed_off = placement.clone().into_iter().collect::<HashSet<_>>();
    not_signed_off.remove(&ctx.author());

    // remove "please react below..." and button
    waited.unwrap().create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .embed(base_embed(ctx)
                .description(emb_desc))
            .components(vec![
                CreateActionRow::Buttons(vec![
                    initial_confirm_button.disabled(true)])])
    )).await?;

    // part 3: parties to game must sign
    let make_signoff_msg = |not_signed_off: &HashSet<User>, disable_button: bool| (
        format!(
            "please sign off on this game with :white_check_mark:\n\
            simple majority is required to submit game\n\
            {}\n\
            \n\
            ~~struck through~~ players have already signed\n\
            **after 5 minutes of inactivity, game is rejected for submission**",
            participants_friendly.iter().map(|(user, _, _)| {
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

    while not_signed_off.len() >= ((placement.len() / 2) as f32).ceil() as usize {
        let not_signed_off_freeze = not_signed_off.clone();
        match party_sign_stage_msg.await_component_interaction(&ctx.serenity_context().shard)
            .filter(move |ixn| {
                not_signed_off_freeze.contains(&ixn.user)
            })
            .custom_ids(vec![String::from("postgame_party_sign")])
            // TODO: change back
            .timeout(Duration::from_secs(5))
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

    // increment, but the previous value is what we'll use
    // big idea is to prevent someone else from messing with us, so reserve then use
    let avail_game_id = ctx.data().mongo.collection::<LeagueInfo>("league_info").find_one_and_update(
        doc! {},
        doc! { "$inc": doc! { "last_not_submitted": 1 } })
        .await?
        .expect("league_info struct missing")
        .last_not_submitted;

    let signed_game = Game {
        _id: avail_game_id,
        participants: participants_friendly.iter().map(|(_, _, player_id)| *player_id).collect_vec(),
        length: time_seconds,
        when: submitted_time.into(),
        approver: None,
    };

    ctx.data().mongo.collection::<Game>("games").insert_one(signed_game).await?;

    let (_, signoff_components) = make_signoff_msg(&not_signed_off, true);
    party_sign_stage_msg.edit(
        ctx.http(),
        EditMessage::new()
            .components(signoff_components)).await?;

    // part 4: moderator must sign
    ctx.send(CreateReply::default().content(format!(
        "ok, game with ID {avail_game_id} submitted for moderator verification\n\
        **any moderator, please approve or reject this game with `/approve {avail_game_id}`.**",
    )))
        .await?;

    Ok(())
}

#[poise::command(slash_command, prefix_command, check = league_moderators)]
pub(crate) async fn approve(ctx: Context<'_>, game_id: GameID) -> Result<(), BotError> {
    let found = ctx.data().mongo.collection::<Game>("games").find_one(doc! { "_id": game_id.to_string() }).await?;
    if found.is_none() {
        ctx.send(CreateReply::default()
            .content(":x: that game DNE")
            .ephemeral(true)).await?;
        return Ok(());
    }

    if found.unwrap().approver.is_some() {
        ctx.send(CreateReply::default()
            .content(":x: that game already approved")
            .ephemeral(true)).await?;
        return Ok(());
    }

    // TODO
    Err("approve command not done yet".into())
}
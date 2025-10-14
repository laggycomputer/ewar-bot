pub(crate) mod checks;
pub(crate) mod rating;
pub(crate) mod constants;
pub(crate) mod paginate;
pub(crate) mod serialization;

use crate::commands::ewar::user::try_lookup_player;
use crate::commands::ewar::user::UserLookupType::SystemID;
use crate::model::{ApprovalStatus, Player, StandingEvent, StandingEventInner};
use crate::{BotError, Context};
use chrono::Utc;
use discord_md::generate::{ToMarkdownString, ToMarkdownStringOption};
use itertools::Itertools;
use mongodb::Database;
use serenity::all::{CreateEmbed, CreateEmbedAuthor, Mentionable, Permissions};
use serenity::all::{User, UserId};
use timeago::TimeUnit::Seconds;

pub(crate) fn bot_invite_url(id: UserId, permissions: Permissions, with_slash_commands: bool) -> String {
    let perms_section = permissions.bits().to_string();
    format!("https://discord.com/oauth2/authorize?client_id={id}&permissions={perms_section}&integration_type=0&scope=bot{}",
            if with_slash_commands { "+applications.commands" } else { "" })
}

pub(crate) fn remove_markdown(input: &str) -> String {
    let doc = discord_md::parse(input);

    doc.to_markdown_string(&ToMarkdownStringOption::new().omit_format(true))
}

pub(crate) fn base_embed(ctx: Context<'_>) -> CreateEmbed {
    CreateEmbed::default()
        .color(0xfcc11b)
        .author(CreateEmbedAuthor::from(
            User::from(ctx.serenity_context().cache.current_user().clone())))
}

impl Player {
    pub(crate) fn short_summary(&self) -> Box<str> {
        match self.discord_ids.get(0) {
            None => self.username.clone(),
            Some(discord_id) => UserId::try_from(*discord_id).unwrap().mention().to_string(),
        }.into_boxed_str()
    }

    pub(crate) fn reference_no_discord(&self) -> Box<str> {
        format!("{}, ID {}", remove_markdown(&*self.username), self._id).to_owned().into_boxed_str()
    }
}


impl StandingEvent {
    pub(crate) async fn short_summary(&self, mongo: &Database) -> Result<Box<str>, BotError> {
        let summary = match &self.inner {
            StandingEventInner::GameEnd(game) => {
                let mut looked_up = Vec::with_capacity(game.ranking.len());
                for player_id in game.ranking.iter() {
                    looked_up.push(try_lookup_player(mongo, SystemID(*player_id)).await?.expect("user in game not found"));
                }

                let placement_string = {
                    let users = looked_up.into_iter().map(|u| u.short_summary()).collect_vec();

                    if users.len() > 7 {
                        users[..7].join(", ") + ", ..."
                    } else {
                        users.join(", ")
                    }
                };

                let mut time_formatter = timeago::Formatter::new();
                time_formatter
                    .num_items(2)
                    .min_unit(Seconds);

                format!(
                    "game ID {} on <t:{}:d> ({}): {}",
                    game.game_id,
                    self.when.timestamp(),
                    time_formatter.convert_chrono(self.when, Utc::now()),
                    placement_string,
                )
            }
            StandingEventInner::JoinLeague { victims, initial_rating, initial_deviation } => {
                let mut looked_up = Vec::with_capacity(victims.len());
                for player_id in victims.iter() {
                    looked_up.push(try_lookup_player(mongo, SystemID(*player_id)).await?.expect("user joined to league not found"));
                }

                format!(
                    "{} joined league with rating {initial_rating}, deviation {initial_deviation}",
                    looked_up.into_iter().map(|u| u.short_summary()).join(", "))
            }
            StandingEventInner::Penalty { victims, delta_rating, reason } => {
                let mut looked_up = Vec::with_capacity(victims.len());
                for player_id in victims.iter() {
                    looked_up.push(try_lookup_player(mongo, SystemID(*player_id)).await?.expect("penalized user not found"));
                }

                format!("{} penalized {} rating for {reason}",
                        looked_up.into_iter().map(|u| u.short_summary()).join(", "),
                        -delta_rating)
            }
            StandingEventInner::InactivityDecay { victims, delta_deviation } => {
                let mut looked_up = Vec::with_capacity(victims.len());
                for player_id in victims.iter() {
                    looked_up.push(try_lookup_player(mongo, SystemID(*player_id)).await?.expect("penalized user not found"));
                }

                format!("{} gain {} deviation due to 1 week of inactivity",
                        looked_up.into_iter().map(|u| u.short_summary()).join(", "),
                        delta_deviation)
            }
            _ => String::from("don't know how to summarize this event type")
        };

        Ok((if self.approval_status.as_ref()
            .is_some_and(|st| !st.approved) { format!("~~{summary}~~") } else { summary }).into_boxed_str())
    }
}

impl ApprovalStatus {
    pub(crate) async fn short_summary(&self, mongo: &Database) -> Result<Box<str>, BotError> {
        Ok(format!("{} by {}", match self.approved {
            true => "approved",
            false => "rejected"
        }, match self.reviewer {
            Some(reviewer_id) => try_lookup_player(mongo, SystemID(reviewer_id))
                .await?
                .expect("reviewer's ID not valid")
                .short_summary(),
            None => "<system>".into()
        }).into_boxed_str())
    }
}
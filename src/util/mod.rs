pub(crate) mod checks;
pub(crate) mod rating;
pub(crate) mod constants;

use crate::commands::ewar::user::try_lookup_user;
use crate::commands::ewar::user::UserLookupType::SystemID;
use crate::model::{PlayerID, StandingEvent, StandingEventInner};
use crate::{BotError, Context};
use chrono::Utc;
use discord_md::generate::{ToMarkdownString, ToMarkdownStringOption};
use itertools::Itertools;
use serenity::all::{CreateEmbed, CreateEmbedAuthor, Mentionable, Permissions};
use serenity::all::{User, UserId};
use timeago::TimeUnit::Seconds;

pub(crate) fn bot_invite_url(id: UserId, permissions: Permissions, with_slash_commands: bool) -> String {
    let perms_section = permissions.bits().to_string();
    format!("https://discord.com/oauth2/authorize?client_id={}&permissions={}&integration_type=0&scope=bot{}",
            id,
            perms_section,
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

pub(crate) fn short_user_reference(handle: &str, id: PlayerID) -> Box<str> {
    format!("{}, ID {}", remove_markdown(handle), id).to_owned().into_boxed_str()
}

impl StandingEvent {
    pub(crate) async fn short_summary(&self, pg_conn: &deadpool_postgres::Object) -> Result<Box<str>, BotError> {
        match &self.inner {
            StandingEventInner::GameEnd(game) => {
                let mut looked_up = Vec::with_capacity(game.ranking.len());
                for player_id in game.ranking.iter() {
                    looked_up.push(try_lookup_user(pg_conn, SystemID(*player_id)).await?.expect("user in game not found"));
                }

                let placement_string = {
                    let users = looked_up.into_iter().map(|u| match u.discord_ids.get(0) {
                        None => u.handle,
                        Some(discord_id) => discord_id.mention().to_string().into_boxed_str(),
                    }).collect_vec();

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

                Ok(format!(
                    "game ID {} on <t:{}:d> ({}): {}",
                    game.game_id,
                    self.when.timestamp(),
                    time_formatter.convert_chrono(self.when, Utc::now()),
                    placement_string,
                ).into_boxed_str())
            }
            _ => Ok(Box::from("don't know how to summarize this event type"))
        }
    }
}

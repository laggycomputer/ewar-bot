pub(crate) mod checks;
pub(crate) mod rating;
pub(crate) mod constants;

use crate::model::PlayerID;
use crate::Context;
use discord_md::generate::{ToMarkdownString, ToMarkdownStringOption};
use serenity::all::{CreateEmbed, CreateEmbedAuthor, Permissions};
use serenity::all::{User, UserId};

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

pub(crate) fn short_user_reference(handle: &str, id: PlayerID) -> String {
    format!("{}, ID {}", remove_markdown(handle), id)
}

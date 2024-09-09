pub(crate) mod checks;
pub(crate) mod rating;

use crate::Context;
use discord_md::generate::{ToMarkdownString, ToMarkdownStringOption};
use serenity::all::{CreateEmbed, CreateEmbedAuthor, Permissions};
use serenity::all::{User, UserId};

pub(crate) fn bot_invite_url(id: UserId, permissions: Permissions, with_slash_commands: bool) -> String {
    let perms_section = permissions.bits().to_string();
    format!("https://discord.com/oauth2/authorize?client_id={}{}&scope=bot{}",
            id,
            perms_section,
            if with_slash_commands { "%20applications.commands" } else { "" })
}

pub(crate) fn remove_markdown(input: String) -> String {
    let doc = discord_md::parse(&*input);

    doc.to_markdown_string(&ToMarkdownStringOption::new().omit_format(true))
}

pub(crate) fn base_embed(ctx: Context<'_>) -> CreateEmbed {
    CreateEmbed::default()
        .color(0xfcc11b)
        .author(CreateEmbedAuthor::from(
            User::from(ctx.serenity_context().cache.current_user().clone())))
}
use discord_md::generate::{ToMarkdownString, ToMarkdownStringOption};
use serenity::all::Permissions;
use serenity::all::UserId;

pub(crate) fn bot_invite_url(id: UserId, permissions: Permissions, with_slash_commands: bool) -> String {
    let perms_section = permissions.bits().to_string();
    format!("https://discord.com/oauth2/authorize?client_id={}{}&scope=bot{}",
            id,
            perms_section,
            if with_slash_commands { "%20applications.commands" } else { "" })
}

pub(crate) fn remove_markdown(input: String) -> String  {
    let doc = discord_md::parse(&*input);

    doc.to_markdown_string(&ToMarkdownStringOption::new().omit_format(true))
}
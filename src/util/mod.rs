use serenity::all::Permissions;
use serenity::all::UserId;

pub(crate) fn bot_invite_url(id: UserId, permissions: Permissions, with_slash_commands: bool) -> String {
    let perms_section = permissions.bits().to_string();
    format!("https://discord.com/oauth2/authorize?client_id={}{}&scope=bot{}",
            id,
            perms_section,
            if with_slash_commands { "%20applications.commands" } else { "" })
}
use crate::util::{base_embed, remove_markdown};
use crate::{BotError, Context};
use chrono::Utc;
use gix::ThreadSafeRepository;
use itertools::Itertools;
use poise::CreateReply;

/// Check bot is alive, get numerical ping to Discord
#[poise::command(slash_command, prefix_command)]
pub(crate) async fn ping(ctx: Context<'_>) -> Result<(), BotError> {
    let ping_num = ctx.ping().await.as_millis();
    ctx.say(match ping_num {
        0 => String::from("ok, waiting for more data to report ping"),
        _ => format!("hi, heartbeat is pinging in {} ms", ping_num),
    }).await?;
    Ok(())
}

/// See recent Git commits to the bot
#[poise::command(slash_command, prefix_command)]
pub(crate) async fn git(ctx: Context<'_>) -> Result<(), BotError> {
    let recents = {
        let repo = ThreadSafeRepository::open(".")?.to_thread_local();

        let walk = repo.rev_walk([repo.head_id()?]).first_parent_only().all()?
            .take(6)
            .collect_vec();

        let mut ret = Vec::with_capacity(6);
        for commit in walk {
            let commit = commit?;
            let commit_id = commit.id;
            let found = repo.find_commit(commit_id)?;
            let decoded = found.decode()?;
            ret.push((
                commit_id.to_hex().to_string().into_boxed_str(),
                decoded.message().title.to_string().into_boxed_str(),
                decoded.author.time.seconds));
        }

        ret
    };

    let mut time_formatter = timeago::Formatter::new();
    time_formatter.num_items(2);

    ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .description(recents.into_iter()
                .map(|(hash, message, ts)| {
                    let message = String::from(message.trim());

                    format!("`{}` {} ({})", &hash[..6], remove_markdown(&*message), time_formatter.convert_chrono(
                        chrono::DateTime::from_timestamp(ts, 0).unwrap(), Utc::now()
                    ))
                })
                .join("\n")
            ))).await?;

    Ok(())
}

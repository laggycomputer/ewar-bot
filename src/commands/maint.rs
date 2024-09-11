use crate::model::{EventNumber, LeagueInfo, StandingEvent};
use crate::util::rating::advance_approve_pointer;
use crate::{BotError, Context};
use bson::Bson::Int64;
use bson::{doc, Bson, Document};
use futures::TryStreamExt;
use itertools::Itertools;
use prettytable::{format, Row, Table};
use serde::de::DeserializeOwned;
use std::error::Error;
use tokio::time::Instant;
use tokio_postgres::types::Type;

fn get_null_string() -> String {
    String::from("NULL")
}

#[poise::command(prefix_command, slash_command, owners_only)]
pub(crate) async fn sql(ctx: Context<'_>, query: String) -> Result<(), BotError> {
    let pg_conn = ctx.data().postgres.get().await?;

    let start = Instant::now();
    let result = pg_conn.query(&query, &[]).await;
    let elapsed = start.elapsed();

    match result {
        Err(err) => {
            ctx.reply(format!("fail in {}ms:\n{err}", elapsed.as_millis())).await?;
        }
        Ok(rows) => {
            if rows.is_empty() {
                ctx.reply(format!("nothing back in {} ms", elapsed.as_millis())).await?;
                return Ok(());
            }

            let mut table = Table::new();
            table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

            table.set_titles(Row::new(
                rows[0].columns().iter()
                    .map(|col| prettytable::Cell::new(col.name()))
                    .collect_vec()
            ));

            rows.iter().for_each(|row| {
                table.add_row(Row::new(
                    (0..row.len())
                        .map(|ind| {
                            let col_type = row.columns()[ind].type_();

                            prettytable::Cell::new(&(match col_type {
                                &Type::VARCHAR => row.get::<usize, Option<String>>(ind).unwrap_or_else(get_null_string),
                                &Type::INT8 => row.get::<usize, Option<i64>>(ind).as_ref().map(ToString::to_string).unwrap_or_else(get_null_string),
                                &Type::INT4 => row.get::<usize, Option<i32>>(ind).as_ref().map(ToString::to_string).unwrap_or_else(get_null_string),
                                &Type::INT2 => row.get::<usize, Option<i16>>(ind).as_ref().map(ToString::to_string).unwrap_or_else(get_null_string),
                                &Type::FLOAT8 => row.get::<usize, Option<f64>>(ind).as_ref().map(ToString::to_string).unwrap_or_else(get_null_string),
                                &Type::TIMESTAMP => row.get::<usize, Option<chrono::NaiveDateTime>>(ind).as_ref().map(ToString::to_string).unwrap_or_else(get_null_string),
                                &Type::BOOL => row.get::<usize, Option<bool>>(ind).as_ref().map(ToString::to_string).unwrap_or_else(get_null_string),
                                _ => format!("type {col_type} not yet implemented for printing")
                            })
                                .into_boxed_str())
                        })
                        .collect_vec()
                ));
            });

            ctx.reply(format!("ok in {}ms:```\n{table}```", elapsed.as_millis())).await?;
        }
    }
    Ok(())
}

/// attempt to advance the approve pointer (be careful)
#[poise::command(prefix_command, slash_command, owners_only)]
pub(crate) async fn advance_pointer(
    ctx: Context<'_>,
    #[description = "do not approve this event number and after"] stop_before: Option<EventNumber>,
) -> Result<(), BotError> {
    let LeagueInfo { first_unreviewed_event_number, .. } = ctx.data().mongo
        .collection::<LeagueInfo>("league_info")
        .find_one(doc! {})
        .await?
        .expect("league_info struct missing");

    let stopped_before = first_unreviewed_event_number;
    let new_stopped_before = advance_approve_pointer(&ctx.data(), stop_before).await?;

    ctx.reply(match stopped_before == new_stopped_before {
        true => {
            format!("ok, stopped at event number {} (no change)", stopped_before)
        }
        false => {
            format!("ok, previously was stopped before event number {}, now stopped before event number {}",
                    stopped_before,
                    new_stopped_before)
        }
    }).await?;

    Ok(())
}

/// move the advance pointer back to 0
#[poise::command(prefix_command, slash_command, owners_only)]
pub(crate) async fn force_reprocess(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.data().mongo
        .collection::<LeagueInfo>("league_info")
        .find_one_and_update(doc! {}, doc! { "$set": doc! {"first_unreviewed_event_number": Int64(0) } })
        .await?
        .expect("league_info struct missing");

    ctx.reply("ok").await?;

    Ok(())
}

fn try_make<T>(doc: Document) -> Result<T, Box<dyn Error>>
where
    T: DeserializeOwned,
{
    let parsed: T = bson::from_document(doc)?;
    Ok(parsed)
}

/// check integrity of event log
#[poise::command(prefix_command, slash_command, owners_only)]
pub(crate) async fn fsck(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.defer().await?;

    let mut had_err = false;

    let mut events = ctx.data().mongo.collection::<Document>("events").find(doc! {}).await?;
    while let Some(out) = events.try_next().await? {
        let to_send = match try_make::<StandingEvent>(out.clone()) {
            Ok(_) => continue,
            Err(e) => {
                let offender: &Bson = out.get("_id").expect("how does a mongo object not have an id");
                format!("event {} is not okay:\n{:?}", offender.to_string(), e)
            }
        };
        // we will never be here if everything is okay
        had_err = true;
        ctx.reply(to_send).await?;
    }

    if !had_err {
        ctx.reply("all events look ok").await?;
    }
    Ok(())
}

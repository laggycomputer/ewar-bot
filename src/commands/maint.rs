use crate::{BotError, Context};
use itertools::Itertools;
use prettytable::{format, Row, Table};
use tokio::time::Instant;
use tokio_postgres::types::Type;

#[poise::command(slash_command, prefix_command, owners_only)]
pub(crate) async fn sql(ctx: Context<'_>, query: String) -> Result<(), BotError> {
    let conn = ctx.data().postgres.get().await?;

    let start = Instant::now();
    let result = conn.query(&query, &[]).await;
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
                                &Type::VARCHAR => row.get::<usize, String>(ind),
                                &Type::INT8 => row.get::<usize, i64>(ind).to_string(),
                                &Type::INT4 => row.get::<usize, i32>(ind).to_string(),
                                &Type::INT2 => row.get::<usize, i16>(ind).to_string(),
                                &Type::FLOAT8 => row.get::<usize, f64>(ind).to_string(),
                                &Type::TIMESTAMP => row.get::<usize, chrono::NaiveDateTime>(ind).to_string(),
                                &Type::BOOL => row.get::<usize, bool>(ind).to_string(),
                                _ => String::from(format!("type {col_type} not yet implemented for printing"))
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
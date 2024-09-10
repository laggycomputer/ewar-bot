use crate::{BotError, Context};
use itertools::Itertools;
use prettytable::{format, Row, Table};
use tokio::time::Instant;
use tokio_postgres::types::Type;

fn get_null_string() -> String{
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
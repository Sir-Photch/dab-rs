use std::error::Error;
use log::error;
use mysql_async::prelude::*;

#[derive(Debug, PartialEq, Eq, Clone)]
struct GuildDetails {
    id: u64,
    chime_duration_max_ms: Option<u64>,
    blocked_role_id: Option<u64>,
}
impl FromRow for GuildDetails {
    fn from_row_opt(row: mysql_async::Row) -> Result<Self, mysql_async::FromRowError>
    where
        Self: Sized,
    {
        let (id, chime_duration_max_ms, blocked_role_id) =
            mysql_async::from_row_opt::<(u64, Option<u64>, Option<u64>)>(row)?;

        Ok(GuildDetails {
            id,
            chime_duration_max_ms,
            blocked_role_id,
        })
    }
}

#[derive(Clone)]
struct DatabaseInterface {
    pool: mysql_async::Pool,
}
impl DatabaseInterface {
    async fn get_guild_details(&self, guild_id: u64) -> Option<GuildDetails> {
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|err| {
                error!("Could not get connection from pool: {err:?}");
                err
            })
            .ok()?;

        conn.query_first(format!(
            "SELECT id, chime_duration_max_ms, blocked_role_id FROM GuildDetails WHERE id = {}",
            guild_id
        ))
        .await
        .map_err(|err| {
            error!("Could not get details for guild_id {guild_id} from database: {err:?}");
            err
        })
        .ok()?
    }

    async fn set_guild_details(&self, details : GuildDetails) -> Result<(), Box<dyn Error>> {
        let mut conn = self.pool.get_conn().await?;

        conn.exec_drop(format!("")).await
    }
}

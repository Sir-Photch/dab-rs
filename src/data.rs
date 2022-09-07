use log::error;
use mysql_async::prelude::*;
use nameof::name_of;

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct GuildDetails {
    pub id: u64,
    pub chime_duration_max_ms: Option<u64>,
    pub blocked_role_id: Option<u64>,
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
trait TableSchema {
    fn get_schema() -> String;
}
impl TableSchema for GuildDetails {
    fn get_schema() -> String {
        format!(
            r"{} UNSIGNED BIGINT PRIMARY KEY,
              {} UNSIGNED BIGINT,
              {} UNSIGNED BIGINT",
            name_of!(id in GuildDetails),
            name_of!(chime_duration_max_ms in GuildDetails),
            name_of!(blocked_role_id in GuildDetails)
        )
    }
}

#[derive(Clone)]
pub struct DatabaseInterface {
    pool: mysql_async::Pool,
}
impl DatabaseInterface {
    pub fn new(pool: mysql_async::Pool) -> Self {
        DatabaseInterface { pool }
    }

    pub async fn ensure_table_exists(&self, table_name: &str) {
        let mut conn = self
            .pool
            .get_conn()
            .await
            .expect("Could not get conn to ensure table schema");

        conn.query_drop(format!(
            "CREATE TABLE IF NOT EXISTS {} ({});",
            table_name,
            GuildDetails::get_schema()
        ))
        .await
        .expect("Query ensuring table exists failed!");
    }

    pub async fn get_guild_details(&self, guild_id: &u64) -> Option<GuildDetails> {
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

    pub async fn set_guild_details(&self, details: GuildDetails) -> Result<(), mysql_async::Error> {
        let mut conn = self.pool.get_conn().await?;

        conn.query_drop(format!(
            "REPLACE INTO GuildDetails VALUES ({},{},{})",
            details.id,
            details
                .chime_duration_max_ms
                .map_or("NULL".to_string(), |ms| ms.to_string()),
            details
                .blocked_role_id
                .map_or("NULL".to_string(), |id| id.to_string())
        ))
        .await?;

        Ok(())
    }

    pub async fn disconnect(self) -> Result<(), mysql_async::Error> {
        self.pool.disconnect().await?;

        Ok(())
    }
}

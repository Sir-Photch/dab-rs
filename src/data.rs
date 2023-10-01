use log::error;
use nameof::name_of;

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct GuildDetails {
    pub id: u64,
    pub blocked_role_id: Option<u64>,
}
trait TableSchema {
    fn get_schema() -> String;
}
impl TableSchema for GuildDetails {
    fn get_schema() -> String {
        format!(
            "{} BIGINT PRIMARY KEY, {} BIGINT",
            name_of!(id in GuildDetails),
            name_of!(blocked_role_id in GuildDetails)
        )
    }
}

pub struct DatabaseInterface {
    client: tokio_postgres::Client,
    table_name: String,
}
impl DatabaseInterface {
    pub fn new(client: tokio_postgres::Client, table_name: &str) -> Self {
        DatabaseInterface {
            client,
            table_name: table_name.to_owned(),
        }
    }

    pub async fn ensure_table_exists(&self) {
        self.client
            .execute_raw::<_, _, &[&str]>(
                &format!(
                    "CREATE TABLE IF NOT EXISTS {} ({})",
                    self.table_name,
                    GuildDetails::get_schema()
                ),
                &[],
            )
            .await
            .expect("Query ensuring table exists failed!");
    }

    pub async fn get_guild_details(&self, guild_id: &u64) -> Option<GuildDetails> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "SELECT {}, {} FROM {} WHERE id = $1::BIGINT",
                    name_of!(id in GuildDetails),
                    name_of!(blocked_role_id in GuildDetails),
                    self.table_name
                ),
                &[&(*guild_id as i64)],
            )
            .await
            .map_err(|err| {
                error!("Could not get details for guild_id {guild_id} from database: {err:?}");
                err
            })
            .ok()??;

        Some(GuildDetails {
            id: row.get::<usize, i64>(0) as u64,
            blocked_role_id: match row.try_get::<usize, i64>(1) {
                Ok(val) => Some(val as u64),
                Err(_) => None,
            },
        })
    }

    pub async fn set_guild_details(
        &self,
        details: GuildDetails,
    ) -> Result<(), tokio_postgres::Error> {
        self.client
        .execute(
            &format!(
                "INSERT INTO {table} ({key},{value}) VALUES ($1::BIGINT, $2::BIGINT) ON CONFLICT ({key}) DO UPDATE SET {value} = EXCLUDED.{value}", 
                table = self.table_name,
                key = name_of!(id in GuildDetails),
                value = name_of!(blocked_role_id in GuildDetails)
            ),
            &[
                &(details.id as i64),
                &details.blocked_role_id.map(|unsigned| unsigned as i64)
            ]
        ).await?;

        Ok(())
    }
}

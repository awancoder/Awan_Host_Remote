//! Read-only database browsing inside a MariaDB/MySQL or PostgreSQL
//! container, for the Database Browser page. Queries run through
//! `docker exec` with the container's own credentials variables (as the
//! Docker page's backup does), so no password is typed, shown, or stored
//! here. Only fixed SELECTs are sent: list databases, list tables, and one
//! page of a table's rows; names are always quoted as identifiers.
use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use russh::{Channel, ChannelMsg, client};

use crate::docker::DbKind;

/// Rows per page in the data view.
pub const PAGE: usize = 100;
/// Longest cell text kept; longer values (blobs, big JSON) are cut.
const CELL_MAX: usize = 2000;
const OUTPUT_MAX: usize = 16 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct Target {
    pub container: String,
    pub kind: DbKind,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Table {
    /// PostgreSQL schema; `None` for MariaDB/MySQL (the database is the schema).
    pub schema: Option<String>,
    pub name: String,
    pub view: bool,
    /// The server's row estimate (not an exact count).
    pub estimate: Option<u64>,
}

impl Table {
    pub fn label(&self) -> String {
        match &self.schema {
            Some(schema) if schema != "public" => format!("{schema}.{}", self.name),
            _ => self.name.clone(),
        }
    }

    fn same(&self, other: &Table) -> bool { self.schema == other.schema && self.name == other.name }
}

#[derive(Clone)]
pub enum Query {
    Databases,
    Tables(String),
    Rows { db: String, table: Table, offset: usize },
    /// SQL typed in the SQL tab. `write: false` runs it in a read-only
    /// session; `write: true` (DDL/DML) only after the user confirmed.
    Sql { db: Option<String>, sql: String, write: bool },
}

/// Most rows kept from a SQL tab query's result.
pub const SQL_ROWS_MAX: usize = 1000;
pub const SQL_MAX_BYTES: usize = 256 * 1024;

pub enum SqlResult {
    Grid { columns: Vec<String>, rows: Vec<Vec<Option<String>>>, truncated: bool },
    /// Outcome of a write: affected rows (MariaDB/MySQL) or the command
    /// tags psql printed (PostgreSQL).
    Message(String),
}

/// The first keyword, skipping blanks and `--`, `#`, `/* */` comments.
fn first_keyword(sql: &str) -> String {
    let mut rest = sql;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("--").or_else(|| rest.strip_prefix('#')) {
            rest = after.split_once('\n').map_or("", |(_, next)| next);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, next)| next);
        } else {
            break;
        }
    }
    rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_uppercase()
}

/// Statements that only read; they run in a read-only session.
pub fn is_read_query(sql: &str) -> bool {
    matches!(first_keyword(sql).as_str(), "SELECT" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "WITH" | "VALUES" | "TABLE")
}

/// Statements that delete data or structure, for a stronger warning.
pub fn is_destructive(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    upper.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| matches!(word, "DROP" | "TRUNCATE" | "DELETE" | "ALTER"))
}

/// SQL starters for the SQL tab, written for the container's engine.
pub fn template(kind: DbKind, which: &str, table: Option<(&Table, &[String])>) -> String {
    let name = |table: &Table| match (kind, &table.schema) {
        (DbKind::Postgres, Some(schema)) if schema != "public" => format!("{}.{}", ident(kind, schema), ident(kind, &table.name)),
        _ => ident(kind, &table.name),
    };
    let target = table.map(|(table, _)| name(table)).unwrap_or_else(|| ident(kind, "nama_tabel"));
    let columns: Vec<String> = table.map(|(_, columns)| columns.iter().map(|c| ident(kind, c)).collect()).unwrap_or_default();
    let first = columns.first().cloned().unwrap_or_else(|| ident(kind, "id"));
    match which {
        "create" => match kind {
            DbKind::MySql => "CREATE TABLE `nama_tabel` (\n  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,\n  `name` VARCHAR(255) NOT NULL,\n  `created_at` TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP\n);\n".into(),
            DbKind::Postgres => "CREATE TABLE nama_tabel (\n  id BIGSERIAL PRIMARY KEY,\n  name VARCHAR(255) NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL DEFAULT now()\n);\n".into(),
        },
        "insert" if !columns.is_empty() => format!(
            "INSERT INTO {target} ({})\nVALUES ({});\n",
            columns.join(", "),
            columns.iter().map(|_| "''").collect::<Vec<_>>().join(", "),
        ),
        "insert" => format!("INSERT INTO {target} ({first}, {name})\nVALUES ('', '');\n", name = ident(kind, "name")),
        "update" => format!("UPDATE {target}\nSET {col} = ''\nWHERE {first} = 1;\n", col = columns.get(1).cloned().unwrap_or_else(|| ident(kind, "name"))),
        "delete" => format!("DELETE FROM {target}\nWHERE {first} = 1;\n"),
        _ => format!("SELECT *\nFROM {target}\nLIMIT 100;\n"),
    }
}

pub struct Request {
    pub target: Target,
    pub query: Query,
}

pub struct Rows {
    pub db: String,
    pub table: Table,
    pub offset: usize,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    /// Another page follows (one extra row was returned).
    pub more: bool,
}

pub enum Reply {
    Databases(Vec<String>),
    Tables { db: String, tables: Vec<Table> },
    Rows(Rows),
    Sql { db: Option<String>, write: bool, result: SqlResult },
}

fn quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\"'\"'")) }

fn ident(kind: DbKind, name: &str) -> String {
    match kind {
        DbKind::MySql => format!("`{}`", name.replace('`', "``")),
        DbKind::Postgres => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

fn literal(kind: DbKind, value: &str) -> String {
    match kind {
        DbKind::MySql => format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''")),
        DbKind::Postgres => format!("'{}'", value.replace('\'', "''")),
    }
}

fn valid_name(value: &str) -> bool { !value.is_empty() && value.len() <= 256 && !value.chars().any(|c| c.is_control()) }

/// The client run inside the container. MariaDB/MySQL uses root's password
/// variable (or the app account when only a random root password exists);
/// PostgreSQL uses POSTGRES_USER over the container's local socket.
const MYSQL_CLIENT: &str = r#"u=root; pw="${MARIADB_ROOT_PASSWORD:-${MYSQL_ROOT_PASSWORD:-}}"; f="${MARIADB_ROOT_PASSWORD_FILE:-${MYSQL_ROOT_PASSWORD_FILE:-}}"; if [ -z "$pw" ] && [ -n "$f" ]; then pw=$(cat "$f"); fi; if [ -z "$pw" ] && [ -n "${MARIADB_USER:-${MYSQL_USER:-}}" ]; then u="${MARIADB_USER:-$MYSQL_USER}"; pw="${MARIADB_PASSWORD:-${MYSQL_PASSWORD:-}}"; fi; if command -v mariadb >/dev/null 2>&1; then c=mariadb; else c=mysql; fi; MYSQL_PWD="$pw" exec "$c" -u"$u" --batch --default-character-set=utf8mb4 ${AWAN_DB:+"--database=$AWAN_DB"}"#;
const POSTGRES_CLIENT: &str = r#"PGPASSWORD="${POSTGRES_PASSWORD:-}" exec psql -X -q --csv -v ON_ERROR_STOP=1 -U "${POSTGRES_USER:-postgres}" -d "${AWAN_DB:-${POSTGRES_DB:-${POSTGRES_USER:-postgres}}}""#;
/// Writes: plain output so psql prints its command tags (`INSERT 0 1`), and
/// `-1` runs the whole script as one transaction (all or nothing).
const POSTGRES_WRITE_CLIENT: &str = r#"PGPASSWORD="${POSTGRES_PASSWORD:-}" exec psql -X -1 -v ON_ERROR_STOP=1 -U "${POSTGRES_USER:-postgres}" -d "${AWAN_DB:-${POSTGRES_DB:-${POSTGRES_USER:-postgres}}}""#;

impl Request {
    fn command(&self) -> Result<String> {
        let container = &self.target.container;
        if container.is_empty() || !container.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)) {
            bail!("Nama container tidak valid");
        }
        let (client, db) = match (self.target.kind, &self.query) {
            // SQL tab queries run with the chosen database as the default.
            (DbKind::MySql, Query::Sql { db, .. }) => (MYSQL_CLIENT, db.as_deref()),
            (DbKind::MySql, _) => (MYSQL_CLIENT, None),
            // PostgreSQL connects to one database at a time.
            (DbKind::Postgres, Query::Databases) => (POSTGRES_CLIENT, None),
            (DbKind::Postgres, Query::Tables(db) | Query::Rows { db, .. }) => (POSTGRES_CLIENT, Some(db.as_str())),
            (DbKind::Postgres, Query::Sql { db, write, .. }) => (if *write { POSTGRES_WRITE_CLIENT } else { POSTGRES_CLIENT }, db.as_deref()),
        };
        let env = db.map(|db| format!("-e AWAN_DB={} ", quote(db))).unwrap_or_default();
        Ok(crate::docker::docker_shell(&format!("docker exec -i {env}{} sh -c {}", quote(container), quote(client))))
    }

    fn sql(&self) -> Result<String> {
        let kind = self.target.kind;
        let guard = match kind {
            DbKind::MySql => "SET SESSION TRANSACTION READ ONLY;\n",
            DbKind::Postgres => "SET default_transaction_read_only = on;\n",
        };
        let query = match (&self.query, kind) {
            (Query::Databases, DbKind::MySql) => "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME;".to_owned(),
            (Query::Databases, DbKind::Postgres) => "SELECT datname FROM pg_database WHERE NOT datistemplate AND datallowconn ORDER BY 1;".to_owned(),
            (Query::Tables(db), DbKind::MySql) => {
                if !valid_name(db) { bail!("Nama database tidak valid"); }
                format!("SELECT TABLE_NAME, TABLE_TYPE, TABLE_ROWS FROM information_schema.TABLES WHERE TABLE_SCHEMA = {} ORDER BY TABLE_NAME;", literal(kind, db))
            }
            (Query::Tables(db), DbKind::Postgres) => {
                if !valid_name(db) { bail!("Nama database tidak valid"); }
                "SELECT n.nspname, c.relname, CASE WHEN c.relkind IN ('v','m') THEN 'VIEW' ELSE 'BASE TABLE' END, \
                 CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END \
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE c.relkind IN ('r','p','v','m','f') AND n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%' \
                 ORDER BY 1, 2;".to_owned()
            }
            (Query::Rows { db, table, offset }, _) => {
                if !valid_name(db) || !valid_name(&table.name) || table.schema.as_deref().is_some_and(|s| !valid_name(s)) { bail!("Nama tabel tidak valid"); }
                let from = match (kind, &table.schema) {
                    (DbKind::MySql, _) => format!("{}.{}", ident(kind, db), ident(kind, &table.name)),
                    (DbKind::Postgres, Some(schema)) => format!("{}.{}", ident(kind, schema), ident(kind, &table.name)),
                    (DbKind::Postgres, None) => ident(kind, &table.name),
                };
                format!("SELECT * FROM {from} LIMIT {} OFFSET {offset};", PAGE + 1)
            }
            (Query::Sql { sql, write, .. }, _) => {
                if sql.trim().is_empty() { bail!("Query kosong."); }
                if sql.len() > SQL_MAX_BYTES { bail!("Query melebihi {} KiB.", SQL_MAX_BYTES / 1024); }
                if sql.contains('\0') { bail!("Query berisi karakter tidak valid."); }
                let mut script = sql.trim_end().to_owned();
                // The guard shares the first line so error line numbers
                // still match what the user typed.
                if !*write {
                    return Ok(format!("{} {script}\n", guard.trim_end()));
                }
                if kind == DbKind::MySql {
                    // Batch mode prints no "Query OK"; report the last
                    // statement's affected rows as the final result line.
                    if !script.ends_with(';') { script.push(';'); }
                    script.push_str("\nSELECT ROW_COUNT() AS affected_rows;");
                }
                return Ok(format!("{script}\n"));
            }
        };
        Ok(format!("{guard}{query}\n"))
    }
}

pub async fn execute(channel: &mut Channel<client::Msg>, request: Request) -> Result<Reply> {
    let command = request.command()?;
    let sql = request.sql()?;
    channel.exec(true, command).await?;
    channel.data(sql.as_bytes()).await?;
    channel.eof().await?;
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut status = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if output.len() + data.len() > OUTPUT_MAX { bail!("Hasil melebihi batas 16 MiB."); }
                output.extend_from_slice(&data);
            }
            ChannelMsg::ExtendedData { data, .. } => {
                let room = 16384usize.saturating_sub(errors.len());
                errors.extend_from_slice(&data[..data.len().min(room)]);
            }
            ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    if status != Some(0) {
        let message = String::from_utf8_lossy(&errors).trim().to_string();
        bail!("{}", if message.is_empty() { "Query gagal dijalankan di container.".to_owned() } else { friendly_error(&message) });
    }
    let text = String::from_utf8_lossy(&output);
    if let Query::Sql { db, write, .. } = &request.query {
        let result = match (*write, request.target.kind) {
            (true, DbKind::MySql) => {
                let affected = text.lines().rev().find(|line| !line.trim().is_empty()).and_then(|line| line.trim().parse::<i64>().ok());
                SqlResult::Message(match affected {
                    Some(count) if count > 0 => format!("OK · {count} baris terpengaruh (statement terakhir)"),
                    _ => "OK".into(),
                })
            }
            (true, DbKind::Postgres) => {
                let tags = text.trim();
                SqlResult::Message(if tags.is_empty() { "OK".into() } else { tags.chars().take(4000).collect() })
            }
            (false, kind) => {
                let mut grid = match kind { DbKind::MySql => parse_mysql_batch(&text), DbKind::Postgres => parse_csv(&text) };
                let truncated = grid.rows.len() > SQL_ROWS_MAX;
                grid.rows.truncate(SQL_ROWS_MAX);
                SqlResult::Grid { columns: grid.columns, rows: grid.rows, truncated }
            }
        };
        return Ok(Reply::Sql { db: db.clone(), write: *write, result });
    }
    let grid = match request.target.kind {
        DbKind::MySql => parse_mysql_batch(&text),
        DbKind::Postgres => parse_csv(&text),
    };
    Ok(match request.query {
        Query::Databases => Reply::Databases(grid.rows.into_iter().filter_map(|row| row.into_iter().next().flatten()).collect()),
        Query::Tables(db) => {
            let tables = grid.rows.into_iter().filter_map(|row| {
                let cell = |index: usize| row.get(index).cloned().flatten();
                Some(match request.target.kind {
                    DbKind::MySql => Table { schema: None, name: cell(0)?, view: cell(1).is_some_and(|t| t.contains("VIEW")), estimate: cell(2).and_then(|n| n.parse().ok()) },
                    DbKind::Postgres => Table { schema: cell(0), name: cell(1)?, view: cell(2).is_some_and(|t| t.contains("VIEW")), estimate: cell(3).and_then(|n| n.parse().ok()) },
                })
            }).collect();
            Reply::Tables { db, tables }
        }
        Query::Rows { db, table, offset } => {
            let mut rows = grid.rows;
            let more = rows.len() > PAGE;
            rows.truncate(PAGE);
            Reply::Rows(Rows { db, table, offset, columns: grid.columns, rows, more })
        }
        Query::Sql { .. } => unreachable!("handled above"),
    })
}

/// Turns common client errors into a hint the user can act on.
fn friendly_error(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("permission denied") && lower.contains("docker") {
        return format!("{message}\n\nAkun SSH belum punya akses Docker; atur dari halaman Docker.");
    }
    if lower.contains("access denied") || lower.contains("password authentication failed") {
        return format!("{message}\n\nPassword root/user tidak tersedia di variabel environment container, atau sudah diganti setelah container dibuat.");
    }
    message.to_owned()
}

struct Grid {
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
}

fn clip(mut value: String) -> String {
    if value.len() > CELL_MAX {
        let mut cut = CELL_MAX;
        while !value.is_char_boundary(cut) { cut -= 1; }
        value.truncate(cut);
        value.push('…');
    }
    value
}

/// `mysql --batch`: tab-separated with a header line; `\t`, `\n`, `\\`, `\0`
/// escaped; SQL NULL printed as `NULL`. An empty result prints nothing.
fn parse_mysql_batch(text: &str) -> Grid {
    let unescape = |field: &str| -> String {
        let mut out = String::with_capacity(field.len());
        let mut chars = field.chars();
        while let Some(c) = chars.next() {
            if c != '\\' { out.push(c); continue; }
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        }
        out
    };
    let mut lines = text.lines();
    let columns: Vec<String> = lines.next().map(|line| line.split('\t').map(unescape).collect()).unwrap_or_default();
    let rows = lines.map(|line| line.split('\t').map(|field| if field == "NULL" { None } else { Some(clip(unescape(field))) }).collect()).collect();
    Grid { columns, rows }
}

/// `psql --csv`: RFC 4180 with a header line. An unquoted empty field is
/// SQL NULL; an empty string is written as `""`.
fn parse_csv(text: &str) -> Grid {
    let mut records: Vec<Vec<Option<String>>> = Vec::new();
    let mut record: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let (mut quoted, mut in_quotes, mut at_start) = (false, false, true);
    let mut chars = text.chars().peekable();
    let push_field = |record: &mut Vec<Option<String>>, field: &mut String, quoted: bool| {
        let value = std::mem::take(field);
        record.push(if value.is_empty() && !quoted { None } else { Some(clip(value)) });
    };
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') { chars.next(); field.push('"'); } else { in_quotes = false; }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start => { in_quotes = true; quoted = true; at_start = false; }
            ',' => { push_field(&mut record, &mut field, quoted); quoted = false; at_start = true; }
            '\r' => {}
            '\n' => {
                push_field(&mut record, &mut field, quoted);
                records.push(std::mem::take(&mut record));
                quoted = false;
                at_start = true;
            }
            c => { field.push(c); at_start = false; }
        }
    }
    if !field.is_empty() || !record.is_empty() || quoted {
        push_field(&mut record, &mut field, quoted);
        records.push(record);
    }
    let mut records = records.into_iter();
    let columns = records.next().map(|header| header.into_iter().map(Option::unwrap_or_default).collect()).unwrap_or_default();
    Grid { columns, rows: records.collect() }
}

/// Database Browser state for the active session; reset on disconnect.
#[derive(Default)]
pub struct State {
    pub target: Option<Target>,
    pub databases: Option<Vec<String>>,
    pub tables: HashMap<String, Vec<Table>>,
    pub expanded: HashSet<String>,
    pub selected: Option<(String, Table)>,
    pub offset: usize,
    pub rows: Option<Rows>,
    pub loading: bool,
    /// Queries waiting for the channel, in order. Only the newest page
    /// request is kept, so clicking through tables doesn't pile up queries.
    pub queued: std::collections::VecDeque<Query>,
    pub error: Option<String>,
    /// Bumped when `rows` changes so the page reloads its table view.
    pub revision: u64,
    /// Default database for the SQL tab (the last database clicked).
    pub sql_db: Option<String>,
    pub sql_result: Option<SqlResult>,
    pub sql_error: Option<String>,
    pub sql_running: bool,
    /// Bumped when `sql_result` changes (reloads the SQL result grid).
    pub sql_revision: u64,
    /// The query on the channel is from the SQL tab, so its error belongs
    /// there rather than in the page's notice.
    pub in_flight_sql: bool,
}

impl State {
    /// Opens `target`, keeping what was loaded if it's the same container.
    pub fn open(&mut self, target: Target) {
        if self.target.as_ref() != Some(&target) {
            // `loading` carries over: a query for the previous container may
            // still hold the channel, so new requests queue behind it.
            *self = State {
                target: Some(target), revision: self.revision + 1, sql_revision: self.sql_revision + 1,
                loading: self.loading, in_flight_sql: self.in_flight_sql, ..State::default()
            };
        }
    }

    /// Whether the loaded rows are the selected table's current page.
    pub fn rows_current(&self) -> bool {
        match (&self.rows, &self.selected) {
            (Some(rows), Some((db, table))) => rows.db == *db && rows.table.same(table) && rows.offset == self.offset,
            _ => false,
        }
    }

    /// Applies a reply; returns follow-up queries to send (after a write:
    /// the changed table list and the open page).
    pub fn finish(&mut self, result: Result<Reply, String>) -> Vec<Query> {
        self.loading = false;
        let was_sql = std::mem::take(&mut self.in_flight_sql);
        if was_sql { self.sql_running = false; }
        let mut follow_up = Vec::new();
        match result {
            Ok(Reply::Sql { db, write, result }) => {
                self.sql_result = Some(result);
                self.sql_error = None;
                self.sql_revision += 1;
                // A write can add/drop tables or change rows: re-read them.
                if write {
                    let changed = db.or_else(|| self.selected.as_ref().map(|(db, _)| db.clone()));
                    for db in changed.into_iter().filter(|db| self.expanded.contains(db)) {
                        follow_up.push(Query::Tables(db));
                    }
                    if let Some((db, table)) = self.selected.clone() {
                        follow_up.push(Query::Rows { db, table, offset: self.offset });
                    }
                }
            }
            Err(error) if was_sql => { self.sql_error = Some(error); self.sql_result = None; self.sql_revision += 1; }
            Ok(Reply::Databases(databases)) => { self.databases = Some(databases); self.error = None; }
            Ok(Reply::Tables { db, tables }) => { self.tables.insert(db, tables); self.error = None; }
            Ok(Reply::Rows(rows)) => {
                // A page for a table the user has since left is dropped.
                let wanted = self.selected.as_ref().is_some_and(|(db, table)| rows.db == *db && rows.table.same(table));
                if wanted {
                    self.offset = rows.offset;
                    self.rows = Some(rows);
                    self.revision += 1;
                    self.error = None;
                }
            }
            Err(error) => self.error = Some(error),
        }
        follow_up
    }
}

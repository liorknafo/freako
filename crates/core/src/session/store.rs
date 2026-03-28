use std::path::Path;
use anyhow::{Context, Result};
use rusqlite::Connection;
use super::types::Session;
use crate::memory::store::canonicalize_scope_key;
use crate::skill::store::initialize_schema;

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data dir: {}", data_dir.display()))?;
        let db_path = data_dir.join("sessions.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

        initialize_schema(&conn)?;
        // Migration: add plan columns if they don't exist yet (ignore error if already present)
        let _ = conn.execute("ALTER TABLE sessions ADD COLUMN plan_json TEXT", []);
        let _ = conn.execute("ALTER TABLE sessions ADD COLUMN plan_panel_open INTEGER NOT NULL DEFAULT 0", []);

        Ok(Self { conn })
    }

    pub fn save_session(&self, session: &Session) -> Result<()> {
        let messages_json = serde_json::to_string(&session.messages).context("Failed to serialize messages")?;
        let plan_json = serde_json::to_string(&session.plan_tasks).context("Failed to serialize plan tasks")?;
        let working_directory = canonicalize_scope_key(&session.working_directory);
        self.conn.execute(
            "INSERT OR REPLACE INTO sessions (id, title, created_at, updated_at, working_directory, total_input_tokens, total_output_tokens, messages_json, plan_json, plan_panel_open)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                session.id.to_string(), session.title,
                session.created_at.to_rfc3339(), session.updated_at.to_rfc3339(),
                working_directory, session.total_input_tokens, session.total_output_tokens,
                messages_json, plan_json, session.plan_panel_open as i32,
            ],
        ).context("Failed to save session")?;
        Ok(())
    }

    pub fn list_sessions(&self, working_directory: &str) -> Result<Vec<(String, String, String)>> {
        let working_directory = canonicalize_scope_key(working_directory);
        let mut stmt = self.conn.prepare(
            "SELECT id, title, updated_at FROM sessions WHERE working_directory = ?1 ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map(rusqlite::params![working_directory], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn load_session(&self, id: &str) -> Result<Option<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at, working_directory, total_input_tokens, total_output_tokens, messages_json, plan_json, plan_panel_open FROM sessions WHERE id = ?1"
        )?;
        let result = stmt.query_row(rusqlite::params![id], |row| {
            Ok((
                row.get::<_, String>(0)?, row.get::<_, String>(1)?,
                row.get::<_, String>(2)?, row.get::<_, String>(3)?,
                row.get::<_, String>(4)?, row.get::<_, u32>(5)?,
                row.get::<_, u32>(6)?, row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?, row.get::<_, Option<i32>>(9)?,
            ))
        });
        match result {
            Ok((id_s, title, ca, ua, wd, it, ot, mj, plan_json, plan_panel_open)) => {
                let id = uuid::Uuid::parse_str(&id_s).map_err(|e| anyhow::anyhow!("{}", e))?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&ca)?.with_timezone(&chrono::Utc);
                let updated_at = chrono::DateTime::parse_from_rfc3339(&ua)?.with_timezone(&chrono::Utc);
                let messages = serde_json::from_str(&mj)?;
                let plan_tasks = plan_json
                    .as_deref()
                    .and_then(|j| serde_json::from_str(j).ok())
                    .unwrap_or_default();
                Ok(Some(Session {
                    id, title, created_at, updated_at, messages, working_directory: wd,
                    total_input_tokens: it, total_output_tokens: ot,
                    plan_tasks,
                    plan_panel_open: plan_panel_open.unwrap_or(0) != 0,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM sessions WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }
}

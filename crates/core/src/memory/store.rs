use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::memory::types::{MemoryEntry, MemoryScope};

pub fn canonicalize_scope_key(path: &str) -> String {
    canonicalize_scope_path(path)
        .display()
        .to_string()
}

pub fn canonicalize_scope_path(path: &str) -> PathBuf {
    Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
}

pub struct MemoryStore {
    conn: Connection,
}

impl MemoryStore {
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data dir: {}", data_dir.display()))?;
        let db_path = data_dir.join("sessions.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                scope_type TEXT NOT NULL,
                scope_key TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope_type, scope_key, updated_at DESC);",
        )
        .context("Failed to create memories table")?;

        Ok(Self { conn })
    }

    pub fn list_by_scope(&self, scope: MemoryScope, scope_key: &str) -> Result<Vec<MemoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scope_type, scope_key, title, content, updated_at
             FROM memories
             WHERE scope_type = ?1 AND scope_key = ?2
             ORDER BY updated_at DESC, title ASC",
        )?;

        let rows = stmt
            .query_map(params![scope.as_str(), scope_key], |row| {
                let scope_str: String = row.get(1)?;
                let updated_at: String = row.get(5)?;
                let parsed_scope = MemoryScope::from_str(&scope_str).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Unknown memory scope: {scope_str}"),
                        )),
                    )
                })?;
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;

                Ok(MemoryEntry {
                    id: row.get(0)?,
                    scope: parsed_scope,
                    scope_key: row.get(2)?,
                    title: row.get(3)?,
                    content: row.get(4)?,
                    updated_at,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn list_project_memories(&self, working_directory: &str) -> Result<Vec<MemoryEntry>> {
        self.list_by_scope(MemoryScope::Project, &canonicalize_scope_key(working_directory))
    }

    pub fn list_global_memories(&self) -> Result<Vec<MemoryEntry>> {
        self.list_by_scope(MemoryScope::Global, "global")
    }

    pub fn load_memory(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scope_type, scope_key, title, content, updated_at
             FROM memories
             WHERE id = ?1",
        )?;

        let result = stmt.query_row(params![id], |row| {
            let scope_str: String = row.get(1)?;
            let updated_at: String = row.get(5)?;
            let scope = MemoryScope::from_str(&scope_str).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Unknown memory scope: {scope_str}"),
                    )),
                )
            })?;
            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;

            Ok(MemoryEntry {
                id: row.get(0)?,
                scope,
                scope_key: row.get(2)?,
                title: row.get(3)?,
                content: row.get(4)?,
                updated_at,
            })
        });

        match result {
            Ok(memory) => Ok(Some(memory)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn upsert_memory(
        &self,
        id: &str,
        scope: MemoryScope,
        scope_key: &str,
        title: &str,
        content: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO memories (id, scope_type, scope_key, title, content, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 scope_type = excluded.scope_type,
                 scope_key = excluded.scope_key,
                 title = excluded.title,
                 content = excluded.content,
                 updated_at = excluded.updated_at",
            params![id, scope.as_str(), scope_key, title, content, now],
        )?;
        Ok(())
    }

    pub fn delete_memory(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(())
    }
}

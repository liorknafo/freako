use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::skill::types::{SkillInfo, SkillSourceKind};

pub struct SkillStore {
    conn: Connection,
}

impl SkillStore {
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data dir: {}", data_dir.display()))?;
        let db_path = data_dir.join("sessions.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database: {}", db_path.display()))?;
        initialize_schema(&conn)?;
        Ok(Self { conn })
    }

    pub fn replace_working_dir_skills(&self, working_dir: &str, skills: &[SkillInfo]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM working_dir_skills WHERE working_directory = ?1", params![working_dir])?;

        for skill in skills {
            tx.execute(
                "INSERT INTO skills (name, description, location, content, source_kind, source_url, content_hash, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(location) DO UPDATE SET
                    name = excluded.name,
                    description = excluded.description,
                    content = excluded.content,
                    source_kind = excluded.source_kind,
                    source_url = excluded.source_url,
                    content_hash = excluded.content_hash,
                    updated_at = excluded.updated_at",
                params![
                    skill.name,
                    skill.description,
                    skill.location,
                    skill.content,
                    skill.source_kind.as_str(),
                    skill.source_url,
                    skill.content_hash,
                    skill.updated_at,
                ],
            )?;

            tx.execute(
                "INSERT INTO working_dir_skills (working_directory, skill_location, skill_name)
                 VALUES (?1, ?2, ?3)",
                params![working_dir, skill.location, skill.name],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn clear_working_dir_skills(&self, working_dir: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM working_dir_skills WHERE working_directory = ?1",
            params![working_dir],
        )?;
        Ok(())
    }

    pub fn load_working_dir_skills(&self, working_dir: &str) -> Result<Vec<SkillInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.name, s.description, s.location, s.content, s.source_kind, s.source_url, s.content_hash, s.updated_at
             FROM skills s
             INNER JOIN working_dir_skills w ON w.skill_location = s.location
             WHERE w.working_directory = ?1
             ORDER BY w.skill_name ASC"
        )?;

        let rows = stmt.query_map(params![working_dir], |row| {
            let source_kind: String = row.get(4)?;
            Ok(SkillInfo {
                name: row.get(0)?,
                description: row.get(1)?,
                location: row.get(2)?,
                content: row.get(3)?,
                source_kind: SkillSourceKind::from_str(&source_kind).unwrap_or(SkillSourceKind::LocalPath),
                source_url: row.get(5)?,
                content_hash: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }
}

pub(crate) fn initialize_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            working_directory TEXT NOT NULL,
            total_input_tokens INTEGER DEFAULT 0,
            total_output_tokens INTEGER DEFAULT 0,
            messages_json TEXT NOT NULL DEFAULT '[]'
        );
        CREATE TABLE IF NOT EXISTS skills (
            location TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            content TEXT NOT NULL,
            source_kind TEXT NOT NULL,
            source_url TEXT,
            content_hash TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS working_dir_skills (
            working_directory TEXT NOT NULL,
            skill_location TEXT NOT NULL,
            skill_name TEXT NOT NULL,
            PRIMARY KEY (working_directory, skill_location)
        );
        CREATE INDEX IF NOT EXISTS idx_working_dir_skills_directory ON working_dir_skills (working_directory);
        CREATE INDEX IF NOT EXISTS idx_skills_name ON skills (name);"
    ).context("Failed to create database schema")?;
    Ok(())
}

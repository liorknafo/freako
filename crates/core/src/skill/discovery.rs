use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use sha2::{Digest, Sha256};

use crate::config::types::SkillsConfig;
use crate::skill::store::SkillStore;
use crate::skill::types::{DiscoveredSkill, SkillInfo, SkillSourceKind};

const USER_AGENT: &str = "freako/0.1";

pub async fn discover_and_sync_skills(
    store: &SkillStore,
    working_dir: &str,
    config: &SkillsConfig,
    data_dir: &Path,
) -> Result<Vec<SkillInfo>> {
    let synced = discover_skills(working_dir, config, data_dir).await?;
    sync_working_dir_skills(store, working_dir, config.enabled, &synced)?;
    Ok(synced)
}

pub async fn discover_skills(
    working_dir: &str,
    config: &SkillsConfig,
    data_dir: &Path,
) -> Result<Vec<SkillInfo>> {
    if !config.enabled {
        return Ok(Vec::new());
    }

    let mut discovered = Vec::new();
    let working_path = Path::new(working_dir);

    discovered.extend(discover_local_project_skills(working_path).await?);

    for path in &config.paths {
        discovered.extend(discover_local_config_skills(working_path, path).await?);
    }

    discovered.extend(discover_remote_skills(config, data_dir).await?);

    let mut deduped = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for skill in discovered {
        if seen.insert(skill.info.name.clone()) {
            deduped.push(skill.info);
        }
    }

    Ok(deduped)
}

pub fn sync_working_dir_skills(
    store: &SkillStore,
    working_dir: &str,
    enabled: bool,
    skills: &[SkillInfo],
) -> Result<()> {
    if !enabled {
        store.clear_working_dir_skills(working_dir)?;
        return Ok(());
    }

    store.replace_working_dir_skills(working_dir, skills)
}

pub fn load_skills_for_working_dir(store: &SkillStore, working_dir: &str) -> Result<Vec<SkillInfo>> {
    store.load_working_dir_skills(working_dir)
}

pub fn format_skills_summary(skills: &[SkillInfo], working_dir: &str) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut out = String::from("## Available Skills\n");
    for skill in skills {
        let location = display_location(&skill.location, working_dir);
        out.push_str(&format!("- **{}**: {} (`{}`)\n", skill.name, skill.description, location));
    }
    out.push('\n');
    out
}

pub fn format_skill_detail(skills: &[SkillInfo], working_dir: &str) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut out = String::from("## Skill Content\n\n");
    for skill in skills {
        let location = display_location(&skill.location, working_dir);
        out.push_str(&format!("### {}\n", skill.name));
        out.push_str(&format!("Source: `{}`\n\n", location));
        out.push_str(&skill.content);
        out.push_str("\n\n");
    }
    out
}

async fn discover_local_project_skills(working_path: &Path) -> Result<Vec<DiscoveredSkill>> {
    let mut results = Vec::new();
    for root in [working_path.join("skills"), working_path.join(".freako").join("skills")] {
        if root.is_dir() {
            results.extend(scan_skill_dir(&root, SkillSourceKind::Project).await?);
        }
    }
    Ok(results)
}

async fn discover_local_config_skills(working_path: &Path, path: &PathBuf) -> Result<Vec<DiscoveredSkill>> {
    let root = if path.is_absolute() {
        path.clone()
    } else {
        working_path.join(path)
    };

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    scan_skill_dir(&root, SkillSourceKind::LocalPath).await
}

async fn scan_skill_dir(root: &Path, source_kind: SkillSourceKind) -> Result<Vec<DiscoveredSkill>> {
    let pattern = format!("{}/**/SKILL.md", root.to_string_lossy().replace('\\', "/"));
    let mut results = Vec::new();

    for entry in glob::glob(&pattern).context("Failed to glob skill directory")? {
        let path = match entry {
            Ok(path) => path,
            Err(_) => continue,
        };
        if !path.is_file() {
            continue;
        }
        if let Some(skill) = parse_skill_file(&path, source_kind.clone(), None).await? {
            results.push(skill);
        }
    }

    results.sort_by(|a, b| a.info.name.cmp(&b.info.name));
    Ok(results)
}

async fn discover_remote_skills(config: &SkillsConfig, data_dir: &Path) -> Result<Vec<DiscoveredSkill>> {
    if config.sources.is_empty() {
        return Ok(Vec::new());
    }

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| Client::new());

    let mut results = Vec::new();
    for source in &config.sources {
        let source = source.trim();
        if source.is_empty() {
            continue;
        }
        let mut discovered = fetch_remote_skill_source(&client, source, data_dir).await?;
        results.append(&mut discovered);
    }
    Ok(results)
}

async fn fetch_remote_skill_source(client: &Client, source: &str, data_dir: &Path) -> Result<Vec<DiscoveredSkill>> {
    let (owner, repo) = parse_github_source(source)
        .with_context(|| format!("Unsupported skill source: {}", source))?;

    let api_url = format!("https://api.github.com/repos/{owner}/{repo}/git/trees/HEAD?recursive=1");
    let tree = client
        .get(&api_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .with_context(|| format!("Failed to fetch repository tree: {}", source))?
        .error_for_status()
        .with_context(|| format!("Repository tree request failed: {}", source))?
        .json::<serde_json::Value>()
        .await
        .with_context(|| format!("Failed to parse repository tree: {}", source))?;

    let entries = tree
        .get("tree")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let cache_root = data_dir.join("cache").join("skills").join(hash_text(source));
    std::fs::create_dir_all(&cache_root)
        .with_context(|| format!("Failed to create skill cache dir: {}", cache_root.display()))?;

    let mut results = Vec::new();
    for entry in entries {
        let path = match entry.get("path").and_then(|v| v.as_str()) {
            Some(path) if path.ends_with("SKILL.md") && path.starts_with("skills/") => path,
            _ => continue,
        };

        let raw_url = format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/{path}");
        let cache_path = cache_root.join(path.replace('/', "\\"));
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create skill cache dir: {}", parent.display()))?;
        }

        let content = client
            .get(&raw_url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch remote skill: {}", raw_url))?
            .error_for_status()
            .with_context(|| format!("Remote skill request failed: {}", raw_url))?
            .text()
            .await
            .with_context(|| format!("Failed to read remote skill body: {}", raw_url))?;

        tokio::fs::write(&cache_path, &content)
            .await
            .with_context(|| format!("Failed to cache remote skill: {}", cache_path.display()))?;

        if let Some(skill) = parse_skill_content(
            cache_path.to_string_lossy().as_ref(),
            &content,
            SkillSourceKind::Remote,
            Some(raw_url.clone()),
        )? {
            results.push(DiscoveredSkill {
                info: skill,
                base_dir: cache_path.parent().map(|p| p.to_path_buf()),
            });
        }
    }

    Ok(results)
}

async fn parse_skill_file(path: &Path, source_kind: SkillSourceKind, source_url: Option<String>) -> Result<Option<DiscoveredSkill>> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

    Ok(parse_skill_content(
        path.to_string_lossy().as_ref(),
        &content,
        source_kind,
        source_url,
    )?
    .map(|info| DiscoveredSkill {
        info,
        base_dir: path.parent().map(|p| p.to_path_buf()),
    }))
}

fn parse_github_source(source: &str) -> Option<(String, String)> {
    let source = source.trim().trim_end_matches('/');

    if let Some(rest) = source.strip_prefix("https://github.com/") {
        let mut parts = rest.split('/');
        let owner = parts.next()?;
        let repo = parts.next()?;
        return Some((owner.to_string(), repo.to_string()));
    }

    let mut parts = source.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }

    Some((owner.to_string(), repo.to_string()))
}

fn parse_skill_content(
    location: &str,
    content: &str,
    source_kind: SkillSourceKind,
    source_url: Option<String>,
) -> Result<Option<SkillInfo>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let (meta, body) = parse_frontmatter(trimmed)?;
    let name = match meta.get("name") {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => return Ok(None),
    };
    let description = match meta.get("description") {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => return Ok(None),
    };

    let body = body.trim().to_string();
    if body.is_empty() {
        return Ok(None);
    }

    Ok(Some(SkillInfo {
        name,
        description,
        location: location.to_string(),
        content_hash: hash_text(content),
        content: body,
        source_kind,
        source_url,
        updated_at: Utc::now().to_rfc3339(),
    }))
}

fn parse_frontmatter(content: &str) -> Result<(std::collections::HashMap<String, String>, &str)> {
    let mut meta = std::collections::HashMap::new();
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let frontmatter = &rest[..end];
            let body = &rest[end + 5..];
            for line in frontmatter.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    meta.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
            return Ok((meta, body));
        }
    }
    Ok((meta, content))
}

fn hash_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn display_location(location: &str, working_dir: &str) -> String {
    let path = Path::new(location);
    let working = Path::new(working_dir);
    path.strip_prefix(working)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| location.to_string())
}

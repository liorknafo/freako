pub mod types;
pub mod store;
pub mod discovery;

pub use types::{DiscoveredSkill, SkillInfo, SkillSourceKind};
pub use store::SkillStore;
pub use discovery::{
    discover_and_sync_skills, discover_skills, format_skill_detail, format_skills_summary,
    load_skills_for_working_dir, sync_working_dir_skills,
};

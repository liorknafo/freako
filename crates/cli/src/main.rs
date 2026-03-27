mod tui;

use anyhow::Result;
use clap::Parser;

fn print_tools() -> Result<()> {
    let config = freako_core::config::load_config()?;
    let registry = freako_core::tools::ToolRegistry::default_registry(&config);

    let mut tools = registry.all_tools();
    tools.sort_by(|a, b| a.name().cmp(b.name()));

    println!("Available agent tools:\n");
    for tool in tools {
        let approval = if tool.requires_approval() { "yes" } else { "no" };
        println!("- {}: {} (requires approval: {})", tool.name(), tool.description(), approval);
    }

    Ok(())
}

#[derive(Parser)]
#[command(name = "freako", about = "AI code assistant")]
struct Args {
    /// Working directory
    #[arg(long, default_value = ".")]
    working_dir: String,

    /// Model name override
    #[arg(long)]
    model: Option<String>,

    /// API key override
    #[arg(long)]
    api_key: Option<String>,

    /// API base URL override
    #[arg(long)]
    api_base: Option<String>,

    /// List all tools available to the agent and exit
    #[arg(long)]
    list_tools: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.list_tools {
        return print_tools();
    }

    let mut config = freako_core::config::load_config()?;

    // Apply CLI overrides
    if let Some(model) = args.model {
        config.provider.model = model;
    }
    if let Some(key) = args.api_key {
        // Set API key based on provider type
        match config.provider.provider_type {
            freako_core::config::types::ProviderType::OpenAI => {
                config.provider.openai_api_key = Some(key);
            }
            freako_core::config::types::ProviderType::Anthropic => {
                config.provider.anthropic_api_key = Some(key);
            }
            _ => {}
        }
    }
    if let Some(base) = args.api_base {
        config.provider.openai_api_base = Some(base);
    }

    let working_dir = std::fs::canonicalize(&args.working_dir)?
        .display()
        .to_string();

    tui::run(config, working_dir).await
}

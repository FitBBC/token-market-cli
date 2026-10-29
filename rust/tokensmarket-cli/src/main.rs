use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use tokensmarket_core::{
    adapters,
    auth::TokenMarketClient,
    config::{gateway_api_base_url, list_managed_keys, tool_statuses},
    models::{resolve_model, ToolConfig},
    run_setup,
    verification::{verify_token_market, VerificationTest},
};

#[derive(Debug, Parser)]
#[command(
    name = "tokenmarket",
    version,
    about = "Configure Token Market for popular AI agent tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Authorize Token Market and configure an agent tool
    Setup {
        tool: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
    /// Check Token Market connectivity and local configuration
    Doctor { tool: Option<String> },
    /// Run configured request tests
    Test {
        tool: String,
        #[arg(long)]
        models: bool,
        #[arg(long)]
        chat: bool,
        #[arg(long)]
        stream: bool,
        #[arg(long = "tool-call")]
        tool_call: bool,
    },
    /// List keys managed by this CLI
    Keys,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Setup { tool, model } => {
            let selected_tool = match tool {
                Some(tool) => tool,
                None => default_tool()?,
            };
            let outcome = run_setup(&selected_tool, model.as_deref(), |_| {}).await?;
            println!("\n✅ Token Market configured for {}", outcome.tool_name);
            println!("API Key: {} ({})", outcome.api_key_name, outcome.api_key_id);
            println!("Model: {}", outcome.model);
            println!("Config: {}", outcome.config_path);
            if let Some(backup) = &outcome.backup_path {
                println!("Backup: {backup}");
            }
            println!("\nTest results:");
            for result in &outcome.tests {
                println!(
                    "- {}: {}",
                    result.name,
                    if result.passed { "passed" } else { "failed" }
                );
            }
            if outcome.requires_restart {
                println!("\nNext: restart your agent tool if it is currently running, then select the Token Market model.");
            }
        }
        Commands::Doctor { tool } => {
            println!("Token Market gateway base URL: {}", gateway_api_base_url());
            for status in tool_statuses() {
                if let Some(tool) = &tool {
                    if status.id.to_string() != *tool {
                        continue;
                    }
                }
                let key = tokensmarket_core::config::find_managed_key(status.id)?;
                let config = adapters::get_adapter(status.id).read_config()?;
                println!("\n{}", status.name);
                println!(
                    "- Local setup: {}",
                    if key.is_some() {
                        "registered"
                    } else {
                        "not registered"
                    }
                );
                println!(
                    "- Key id: {}",
                    key.as_ref()
                        .map(|key| key.key_id.clone())
                        .unwrap_or_else(|| "-".into())
                );
                println!(
                    "- Config file: {}",
                    key.as_ref()
                        .map(|key| key.config_path.clone())
                        .unwrap_or(status.config_path)
                );
                println!(
                    "- Config status: {}",
                    if config.is_some() {
                        "valid Token Market entry"
                    } else {
                        "missing Token Market entry"
                    }
                );
                if let Some(config) = config {
                    let protocol = adapters::get_adapter(status.id).protocol;
                    let result =
                        verify_token_market(&config, protocol, &[VerificationTest::Models])
                            .await?
                            .pop()
                            .context("missing verification result")?;
                    println!(
                        "- Model API: {}",
                        if result.passed {
                            "reachable".into()
                        } else {
                            format!(
                                "failed{}",
                                result
                                    .detail
                                    .map(|detail| format!(" ({detail})"))
                                    .unwrap_or_default()
                            )
                        }
                    );
                }
            }
        }
        Commands::Test {
            tool,
            models,
            chat,
            stream,
            tool_call,
        } => {
            let mut tests = Vec::new();
            if models {
                tests.push(VerificationTest::Models);
            }
            if chat {
                tests.push(VerificationTest::Chat);
            }
            if stream {
                tests.push(VerificationTest::Stream);
            }
            if tool_call {
                tests.push(VerificationTest::ToolCall);
            }
            if tests.is_empty() {
                tests = vec![
                    VerificationTest::Models,
                    VerificationTest::Chat,
                    VerificationTest::Stream,
                    VerificationTest::ToolCall,
                ];
            }

            let id = tokensmarket_core::adapters::tool_id(&tool)?;
            let adapter = adapters::get_adapter(id);
            let key = tokensmarket_core::config::find_managed_key(id)?
                .context("No managed configuration found. Run tokenmarket setup.")?;
            let config = adapter
                .read_config()?
                .context("Token Market configuration is missing. Run tokenmarket setup.")?;
            let results = verify_token_market(&config, adapter.protocol, &tests).await?;
            for result in results {
                println!(
                    "{} {}{}",
                    if result.passed { "✓" } else { "✗" },
                    result.name,
                    result
                        .detail
                        .map(|detail| format!(" — {detail}"))
                        .unwrap_or_default()
                );
            }
            let _ = key;
        }
        Commands::Keys => {
            let records = list_managed_keys()?;
            if records.is_empty() {
                println!("No keys have been configured yet.");
            }
            for record in records {
                println!("Tool: {}", record.tool);
                println!("Key name: {}", record.key_name);
                println!("Key id: {}", record.key_id);
                println!("Created: {}", record.created_at);
                println!("Revocable: yes (from the Token Market dashboard)");
                println!();
            }
        }
    }
    Ok(())
}

fn default_tool() -> Result<String> {
    println!("Select a tool:");
    for (index, adapter) in adapters::adapters().iter().enumerate() {
        println!("{}. {}", index + 1, adapter.name);
    }
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        bail!("failed to read tool selection");
    }
    let index: usize = input.trim().parse().context("enter a number")?;
    let selected = adapters::adapters()
        .get(index.checked_sub(1).context("selection out of range")?)
        .context("selection out of range")?
        .id;
    Ok(selected.to_string())
}

#[allow(dead_code)]
fn unused() -> Result<ToolConfig> {
    let _ = resolve_model(None)?;
    let _ = TokenMarketClient::new();
    unimplemented!()
}

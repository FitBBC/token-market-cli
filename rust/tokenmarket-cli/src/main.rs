use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, IsTerminal, Write};
use tokenmarket_core::{
    client::{login_manager, require_session},
    connections::{
        ConnectContext, ConnectionManager, ConnectionTestRequest, CredentialLoss, DisconnectRequest,
    },
    desktop::DesktopSession,
    desktop_workspace::DesktopWorkspace,
    inventory::ToolInventory,
    tool_id,
};

#[derive(Parser)]
#[command(
    name = "tokenmarket",
    version,
    about = "Manage Token Market Agent connections"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Sign in once for both CLI and desktop; no tool Key is issued
    Login {
        #[arg(long)]
        no_browser: bool,
    },
    /// End shared management login; tool configurations and credentials remain
    Logout,
    /// Show or switch the shared current workspace
    Workspace { workspace_id: Option<String> },
    /// Connect a tool without running a paid model test
    #[command(alias = "setup")]
    Connect {
        tool: String,
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        confirm_recovery: bool,
    },
    /// Explicitly update a tool using the current account/workspace
    Reconnect {
        tool: String,
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        confirm_recovery: bool,
        #[arg(long)]
        confirm_rebind: bool,
    },
    /// Restore managed configuration while keeping the saved credential
    Disconnect {
        tool: String,
        #[arg(long, value_parser = ["keep-current", "restore-original"])]
        conflicts: Option<String>,
    },
    /// Show local connection ownership, configured models and test status
    #[command(alias = "doctor")]
    Status { tool: Option<String> },
    /// Run one explicit model test; a small usage charge may apply
    Test {
        tool: String,
        #[arg(long)]
        model: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
fn workspace_client(session: &DesktopSession) -> DesktopWorkspace {
    DesktopWorkspace::new(
        tokenmarket_core::config::console_api_base_url(),
        session.web_origin.clone().unwrap_or_default(),
    )
}
async fn run() -> Result<()> {
    match Cli::parse().command {
        Commands::Login { no_browser } => {
            let login = login_manager();
            if let Some(session) = login.restore().await? {
                println!(
                    "已登录：{} · {}",
                    session.user.name, session.default_workspace.name
                );
                return Ok(());
            }
            let attempt = login.begin_remote().await?;
            println!("请在浏览器完成登录：\n{}", attempt.authorize_url);
            if !no_browser {
                let _ = tokenmarket_core::auth::open_browser(&attempt.authorize_url);
            }
            loop {
                if let Some(session) = login.poll_remote().await? {
                    println!(
                        "已登录：{} · {}",
                        session.user.name, session.default_workspace.name
                    );
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
        Commands::Logout => {
            let remote = login_manager().logout().await?;
            println!("已退出共享登录；已有工具连接仍保留。");
            if !remote {
                println!("服务端退出尚未确认，请检查网络后重试。");
            }
        }
        Commands::Workspace { workspace_id } => {
            let session = require_session().await?;
            let api = workspace_client(&session);
            let snapshot = if let Some(id) = workspace_id {
                api.switch(&session, &id).await?
            } else {
                api.load(&session).await?
            };
            println!(
                "当前空间：{} ({})",
                snapshot.current_workspace.name, snapshot.current_workspace.id
            );
            for workspace in snapshot.switchable_workspaces {
                println!("- {} ({})", workspace.name, workspace.id);
            }
            println!("已有工具仍使用各自接入的计费空间。");
        }
        Commands::Connect {
            tool,
            workspace,
            confirm_recovery,
        } => connect(&tool, workspace, false, confirm_recovery, false).await?,
        Commands::Reconnect {
            tool,
            workspace,
            confirm_recovery,
            confirm_rebind,
        } => connect(&tool, workspace, true, confirm_recovery, confirm_rebind).await?,
        Commands::Disconnect { tool, conflicts } => {
            let tool = tool_id(&tool)?;
            let manager = ConnectionManager::current();
            let record = manager
                .list()?
                .into_iter()
                .find(|record| record.tool == tool)
                .context("该工具尚未接入")?;
            let result = manager
                .disconnect(tool, &record.key_id, None, DisconnectRequest::default())
                .await?;
            if result.state == "conflicts" {
                use tokenmarket_core::connections::{ConflictResolution, RestoreChoice};
                let mut resolutions = Vec::new();
                for conflict in &result.conflicts {
                    let choice = match conflicts.as_deref() {
                        Some("keep-current") => RestoreChoice::KeepCurrent,
                        Some("restore-original") => RestoreChoice::RestoreOriginal,
                        _ if io::stdin().is_terminal() => {
                            loop {
                                print!("文件 {} 的配置项 {} 已被修改：[1] 保留当前值 [2] 恢复接入前的值：", conflict.file + 1, conflict.field + 1); io::stdout().flush()?;
                                let mut answer = String::new(); if io::stdin().read_line(&mut answer)? == 0 { bail!("未确认冲突处理，所有文件已保留") }
                                match answer.trim() { "1" => break RestoreChoice::KeepCurrent, "2" => break RestoreChoice::RestoreOriginal, _ => continue }
                            }
                        }
                        _ => bail!("配置包含接入后的修改，所有文件已保留；请交互运行，或明确指定 --conflicts keep-current / restore-original"),
                    };
                    resolutions.push(ConflictResolution {
                        file: conflict.file,
                        field: conflict.field,
                        choice,
                    });
                }
                let applied = manager
                    .disconnect(
                        tool,
                        &record.key_id,
                        None,
                        DisconnectRequest {
                            revision: result.revision,
                            resolutions,
                        },
                    )
                    .await?;
                if applied.state != "disconnected" {
                    bail!("配置在确认期间发生变化，请重新检查冲突")
                }
            }
            println!("已取消接入；下次可继续接入。");
        }
        Commands::Status { tool } => {
            let manager = ConnectionManager::current();
            for entry in ToolInventory::current().list_with_connections(&manager)? {
                if tool
                    .as_ref()
                    .is_some_and(|tool| tool != &entry.id.to_string())
                {
                    continue;
                }
                print!(
                    "{}：{}",
                    entry.name,
                    if entry.installed {
                        "已安装"
                    } else {
                        "未安装"
                    }
                );
                if let Some(connection) = entry.connection {
                    println!(" · {}", connection.state);
                    if let (Some(owner), Some(space)) =
                        (connection.owner, connection.bound_workspace)
                    {
                        println!(
                            "  账号：{} · 计费空间：{}",
                            owner["name"].as_str().unwrap_or(""),
                            space["name"].as_str().unwrap_or("")
                        );
                    }
                    println!("  模型：{}", connection.models.join(", "));
                } else {
                    println!(" · 尚未接入");
                }
            }
        }
        Commands::Test { tool, model } => {
            let tool = tool_id(&tool)?;
            let manager = ConnectionManager::current();
            let record = manager
                .list()?
                .into_iter()
                .find(|record| record.tool == tool)
                .context("该工具尚未接入")?;
            if model.is_none() && record.models.len() != 1 {
                bail!(
                    "请用 --model 指定一个已配置的模型：{}",
                    record.models.join(", ")
                )
            }
            println!(
                "正在测试现有连接，可能产生少量费用；计费空间：{}",
                record.bound_workspace.name
            );
            let result = manager
                .test_connection(
                    tool,
                    &record.key_id,
                    ConnectionTestRequest {
                        confirmed: true,
                        model,
                    },
                )
                .await?;
            println!(
                "{}：{}",
                if result.passed {
                    "测试通过"
                } else {
                    "测试失败"
                },
                result.detail
            );
            if !result.passed {
                bail!("模型测试未通过")
            }
        }
    }
    Ok(())
}
async fn connect(
    tool: &str,
    workspace: Option<String>,
    reconnect: bool,
    confirmed: bool,
    confirm_rebind: bool,
) -> Result<()> {
    let tool = tool_id(tool)?;
    let session = require_session().await?;
    let api = workspace_client(&session);
    let snapshot = if let Some(id) = workspace {
        api.switch(&session, &id).await?
    } else {
        api.load(&session).await?
    };
    let inventory = ToolInventory::current();
    let manager = ConnectionManager::current();
    let path = inventory.config_path(tool);
    let existing = manager
        .list()?
        .into_iter()
        .find(|record| record.tool == tool);
    if !reconnect && existing.is_some() {
        bail!("该工具已有连接，请使用 tokenmarket reconnect {tool}")
    }
    if reconnect && existing.is_none() {
        bail!("该工具尚未接入，请使用 tokenmarket connect {tool}")
    }
    println!(
        "接入账号：{} · 计费空间：{}",
        session.user.name, snapshot.current_workspace.name
    );
    if let Some(previous) = &existing {
        println!(
            "原连接账号：{} · 原计费空间：{}",
            previous.owner.name, previous.bound_workspace.name
        );
        if previous.owner.id != session.user.id
            || previous.bound_workspace.id != snapshot.current_workspace.id
        {
            let accepted = if confirm_rebind {
                true
            } else if io::stdin().is_terminal() {
                print!("确认将这条工具连接切换到以上接入账号和计费空间？[y/N] ");
                io::stdout().flush()?;
                let mut answer = String::new();
                io::stdin().read_line(&mut answer)?;
                matches!(answer.trim(), "y" | "Y" | "yes")
            } else {
                false
            };
            if !accepted {
                bail!("原连接已保留；确认变更归属时可使用 --confirm-rebind")
            }
        }
    }
    let context = ConnectContext {
        session_token: session.session_token,
        owner_id: session.user.id,
        workspace_id: snapshot.current_workspace.id,
    };
    let result = if let Some(existing) = &existing {
        manager
            .reconnect(
                context.clone(),
                tool,
                &existing.key_id,
                &inventory,
                path.clone(),
            )
            .await
            .map(|outcome| outcome.connection)
    } else {
        manager
            .connect(context.clone(), tool, &inventory, path.clone())
            .await
    };
    let connection = match result {
        Ok(connection) => connection,
        Err(error) => {
            let Some(lost) = error.downcast_ref::<CredentialLoss>() else {
                return Err(error);
            };
            let proceed = if confirmed {
                true
            } else if io::stdin().is_terminal() {
                print!("本机保存的连接信息已丢失。重新接入成功后将替换这条连接的旧凭证，是否继续？[y/N] ");
                io::stdout().flush()?;
                let mut answer = String::new();
                io::stdin().read_line(&mut answer)?;
                matches!(answer.trim(), "y" | "Y" | "yes")
            } else {
                false
            };
            if !proceed {
                bail!("原连接未改变；确认恢复时可使用 --confirm-recovery")
            }
            manager
                .recover_connection(
                    context,
                    tool,
                    existing.as_ref().map(|record| record.key_id.as_str()),
                    &inventory,
                    path,
                    &lost.key_id,
                )
                .await?
                .connection
        }
    };
    println!(
        "接入完成：{}；尚未进行模型测试。",
        connection.models.join(", ")
    );
    Ok(())
}

use tokenmarket_core::{
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

fn environment(root: &std::path::Path, platform: HostPlatform) -> InstallationEnvironment {
    InstallationEnvironment {
        platform,
        home: root.join("home"),
        command_directories: vec![root.join("bin")],
        application_directories: vec![root.join("Applications")],
        executable_extensions: vec![".EXE".into(), ".CMD".into(), ".BAT".into(), ".COM".into()],
    }
}

#[test]
fn missing_tools_offer_official_downloads_and_refuse_connection_even_with_leftover_config() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("home/.dsh/profiles/desktop")).unwrap();
    std::fs::create_dir_all(root.path().join("Applications/WorkBuddy.app")).unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    let entries = inventory.list().unwrap();
    assert_eq!(entries.len(), 6);
    assert_eq!(
        entries.iter().map(|e| e.name).collect::<Vec<_>>(),
        [
            "Claude Code",
            "Codex CLI",
            "WorkBuddy",
            "DeepSeek Harness",
            "OpenClaw",
            "OpenCode"
        ]
    );
    assert!(entries.iter().all(|e| !e.installed && !e.can_connect));
    assert_eq!(
        entries[3].download_url,
        "https://www.deepseek.com/download/"
    );
    for entry in entries {
        assert!(entry.download_url.starts_with("https://"));
        assert!(inventory.require_installed(entry.id).is_err());
    }
    assert!(!root.path().join("home/.tokenmarket").exists());
}

fn executable(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "fixture; never execute").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn command_installation_is_freshly_detected_and_removed_without_running_the_tool() {
    let root = tempfile::tempdir().unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    for (tool, command) in [
        (ToolId::ClaudeCode, "claude"),
        (ToolId::Codex, "codex"),
        (ToolId::WorkBuddy, "workbuddy"),
        (ToolId::DeepSeekHarness, "dsh"),
        (ToolId::OpenClaw, "openclaw"),
        (ToolId::OpenCode, "opencode"),
    ] {
        assert!(inventory.require_installed(tool).is_err());
        let binary = root.path().join("bin").join(command);
        executable(&binary);
        assert!(
            inventory
                .list()
                .unwrap()
                .into_iter()
                .find(|e| e.id == tool)
                .unwrap()
                .can_connect
        );
        inventory.require_installed(tool).unwrap();
        std::fs::remove_file(binary).unwrap();
        assert!(inventory.require_installed(tool).is_err());
    }
}

#[cfg(unix)]
#[test]
fn macos_bundles_require_the_declared_executable() {
    let root = tempfile::tempdir().unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::MacOs));
    for (tool, bundle, binary) in [
        (ToolId::WorkBuddy, "WorkBuddy.app", "Electron"),
        (
            ToolId::DeepSeekHarness,
            "DeepSeek Harness.app",
            "DeepSeek Harness",
        ),
        (ToolId::OpenClaw, "OpenClaw.app", "OpenClaw"),
        (ToolId::OpenCode, "OpenCode.app", "OpenCode"),
    ] {
        let contents = root
            .path()
            .join("Applications")
            .join(bundle)
            .join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(contents.join("Info.plist"), format!(r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>{binary}</string></dict></plist>"#)).unwrap();
        assert!(inventory.require_installed(tool).is_err());
        executable(&contents.join("MacOS").join(binary));
        inventory.require_installed(tool).unwrap();
    }
}

#[test]
fn windows_inventory_recognizes_pathext_launchers_and_installed_desktop_executables() {
    let root = tempfile::tempdir().unwrap();
    let inventory = ToolInventory::new(environment(root.path(), HostPlatform::Windows));
    for (tool, launcher) in [
        (ToolId::ClaudeCode, "CLAUDE.EXE"),
        (ToolId::Codex, "codex.cmd"),
        (ToolId::DeepSeekHarness, "dsh.BAT"),
        (ToolId::OpenClaw, "openclaw.com"),
        (ToolId::OpenCode, "opencode.exe"),
    ] {
        executable(&root.path().join("bin").join(launcher));
        inventory.require_installed(tool).unwrap();
    }
    let desktop = root.path().join("Applications/WorkBuddy/WorkBuddy.exe");
    executable(&desktop);
    inventory.require_installed(ToolId::WorkBuddy).unwrap();
    let harness = root
        .path()
        .join("Applications/DeepSeek Harness/DeepSeek Harness.exe");
    executable(&harness);
    std::fs::remove_file(root.path().join("bin/dsh.BAT")).unwrap();
    inventory
        .require_installed(ToolId::DeepSeekHarness)
        .unwrap();
    // A suffix omitted from PATHEXT must not silently become a command.
    let mut restricted = environment(root.path(), HostPlatform::Windows);
    restricted.executable_extensions = vec![".EXE".into()];
    assert!(ToolInventory::new(restricted)
        .require_installed(ToolId::Codex)
        .is_err());
}

#[test]
fn launch_plan_uses_fresh_windows_cmd_directory_without_interpolating_its_shell_characters() {
    let root = tempfile::tempdir().unwrap();
    let mut old = environment(root.path(), HostPlatform::Windows);
    let newly_installed = root.path().join("new install & plugins");
    executable(&newly_installed.join("OPENCLAW.CMD"));
    assert!(ToolInventory::new(old.clone())
        .launch_plan(ToolId::OpenClaw)
        .is_err());
    old.command_directories.insert(0, newly_installed.clone());
    let plan = ToolInventory::new(old)
        .launch_plan(ToolId::OpenClaw)
        .unwrap();
    assert_eq!(plan.program, "cmd");
    assert_eq!(plan.args, ["/c", "start", "", "cmd", "/k", "openclaw tui"]);
    assert!(plan.environment[0]
        .1
        .starts_with(&newly_installed.to_string_lossy().to_string()));
    assert!(!plan.args.iter().any(|v| v.contains("plugins")));
    assert!(!plan.wait_for_exit);
}

#[cfg(unix)]
#[test]
fn macos_launch_plan_uses_nvm_target_and_quotes_paths_for_terminal_and_applescript() {
    let root = tempfile::tempdir().unwrap();
    let mut env = environment(root.path(), HostPlatform::MacOs);
    let directory = root
        .path()
        .join(".nvm/versions/node/v22 'quoted' & new/bin");
    executable(&directory.join("codex"));
    env.command_directories.insert(0, directory.clone());
    let plan = ToolInventory::new(env).launch_plan(ToolId::Codex).unwrap();
    assert_eq!(plan.program, "osascript");
    assert!(plan.args[3].contains("v22") && plan.args[3].contains("codex"));
    assert!(plan.args[3].contains("'\\\"'\\\"'quoted'\\\"'\\\"'"));
    assert!(plan.args[3].contains("env PATH="));
    assert!(plan.wait_for_exit);
}

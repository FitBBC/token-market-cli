mod support;
use tempfile::tempdir;
use tokenmarket_core::{
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

#[test]
fn workbuddy_inventory_uses_the_installed_product_config_directory_and_explicit_override_precedence(
) {
    // One isolated process test owns these environment variables; no user configuration is opened.
    let names = [
        "WORKBUDDY_CONFIG_PATH",
        "WORKBUDDY_CONFIG_DIR",
        "CODEBUDDY_CONFIG_DIR",
    ];
    let saved: Vec<_> = names.iter().map(|name| std::env::var_os(name)).collect();
    for name in names {
        std::env::remove_var(name);
    }
    let root = tempdir().unwrap();
    for platform in [HostPlatform::MacOs, HostPlatform::Windows] {
        let inventory = ToolInventory::new(InstallationEnvironment {
            platform,
            home: root.path().into(),
            command_directories: vec![],
            application_directories: vec![],
            executable_extensions: vec![".EXE".into()],
        });
        let path = || -> std::path::PathBuf {
            inventory
                .list()
                .unwrap()
                .into_iter()
                .find(|e| e.id == ToolId::WorkBuddy)
                .unwrap()
                .config_path
                .into()
        };
        assert_eq!(path(), root.path().join(".workbuddy").join("models.json"));
        std::env::set_var("CODEBUDDY_CONFIG_DIR", root.path().join("legacy-override"));
        assert_eq!(
            path(),
            root.path().join("legacy-override").join("models.json")
        );
        std::env::set_var(
            "WORKBUDDY_CONFIG_DIR",
            root.path().join("workbuddy-override"),
        );
        assert_eq!(
            path(),
            root.path().join("workbuddy-override").join("models.json")
        );
        std::env::set_var("WORKBUDDY_CONFIG_PATH", root.path().join("explicit.json"));
        assert_eq!(path(), root.path().join("explicit.json"));
        for name in names {
            std::env::remove_var(name);
        }
    }
    for (name, value) in names.into_iter().zip(saved) {
        if let Some(value) = value {
            std::env::set_var(name, value);
        }
    }
}

#[tokio::test]
async fn opencode_uses_existing_jsonc_and_preserves_it_when_platform_is_unavailable() {
    use tokenmarket_core::connections::ConnectContext;
    let root = tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let executable = bin.join("opencode");
    std::fs::write(&executable, "fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let directory = root.path().join(".config/opencode");
    std::fs::create_dir_all(&directory).unwrap();
    let jsonc = directory.join("opencode.jsonc");
    let original = "// native comment\n{\"model\":\"manual\"}";
    std::fs::write(&jsonc, original).unwrap();
    let inventory = ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.path().into(),
        command_directories: vec![bin],
        application_directories: vec![],
        executable_extensions: vec![],
    });
    let entry = inventory
        .list()
        .unwrap()
        .into_iter()
        .find(|e| e.id == ToolId::OpenCode)
        .unwrap();
    assert_eq!(std::path::PathBuf::from(&entry.config_path), jsonc);
    let manager = support::manager(
        root.path().join("state"),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    assert!(manager
        .connect(
            ConnectContext {
                session_token: "desktop".into(),
                owner_id: "u1".into(),
                workspace_id: "42".into()
            },
            ToolId::OpenCode,
            &inventory,
            entry.config_path.into()
        )
        .await
        .is_err());
    assert!(!directory.join("opencode.json").exists());
    assert!(manager.list().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(jsonc).unwrap(), original);
}

#[test]
fn native_application_targets_match_detected_installations_without_executing_them() {
    for platform in [HostPlatform::MacOs, HostPlatform::Windows] {
        let root = tempdir().unwrap();
        let applications = root.path().join("Applications");
        let inventory = ToolInventory::new(InstallationEnvironment {
            platform,
            home: root.path().into(),
            command_directories: vec![],
            application_directories: vec![applications.clone()],
            executable_extensions: vec![".EXE".into()],
        });
        for (tool, name, exe) in [
            (ToolId::WorkBuddy, "WorkBuddy", "WorkBuddy.exe"),
            (
                ToolId::OpenClaw,
                "OpenClaw Companion",
                "OpenClawCompanion.exe",
            ),
            (ToolId::OpenCode, "OpenCode", "OpenCode.exe"),
        ] {
            assert!(inventory.application_path(tool).is_none());
            let target = if platform == HostPlatform::MacOs {
                let bundle = applications.join(format!("{name}.app"));
                let contents = bundle.join("Contents");
                std::fs::create_dir_all(contents.join("MacOS")).unwrap();
                std::fs::write(contents.join("Info.plist"),r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>App</string></dict></plist>"#).unwrap();
                let executable = contents.join("MacOS/App");
                std::fs::write(&executable, "fixture; never execute").unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700))
                        .unwrap();
                }
                bundle
            } else {
                let folder = applications.join(name);
                std::fs::create_dir_all(&folder).unwrap();
                let executable = folder.join(exe);
                std::fs::write(&executable, "fixture; never execute").unwrap();
                executable
            };
            inventory.require_installed(tool).unwrap();
            assert_eq!(inventory.application_path(tool), Some(target));
        }
    }
}

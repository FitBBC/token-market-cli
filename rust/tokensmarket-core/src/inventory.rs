//! Read-only discovery of installed tools. No executable is launched during discovery.
use crate::{
    adapters::{adapters, get_adapter},
    config::ManagedKeyRecord,
    ToolId,
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPlatform {
    MacOs,
    Windows,
    Unix,
}

/// Platform boundary used by the desktop and by isolated filesystem fixtures.
#[derive(Debug, Clone)]
pub struct InstallationEnvironment {
    pub platform: HostPlatform,
    pub home: PathBuf,
    pub command_directories: Vec<PathBuf>,
    pub application_directories: Vec<PathBuf>,
    pub executable_extensions: Vec<String>,
}

impl InstallationEnvironment {
    pub fn current() -> Self {
        let home = crate::config::home_dir();
        let platform = if cfg!(target_os = "macos") {
            HostPlatform::MacOs
        } else if cfg!(windows) {
            HostPlatform::Windows
        } else {
            HostPlatform::Unix
        };
        let mut command_directories: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        command_directories.extend([
            home.join(".local/bin"),
            home.join(".bun/bin"),
            home.join(".npm-global/bin"),
            home.join(".cargo/bin"),
            home.join(".opencode/bin"),
        ]);
        let mut application_directories = vec![];
        if platform == HostPlatform::Windows {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                command_directories.push(PathBuf::from(appdata).join("npm"));
            }
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                let local = PathBuf::from(local);
                command_directories.push(local.join("Microsoft/WinGet/Links"));
                application_directories.extend([local.join("Programs"), local]);
            }
            for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Some(path) = std::env::var_os(variable) {
                    application_directories.push(PathBuf::from(path));
                }
            }
            #[cfg(windows)]
            {
                application_directories.extend(registered_application_directories());
                // Newly installed launchers may update the user's registry PATH
                // after this desktop process was started.
                command_directories.extend(registered_command_directories());
            }
        } else {
            command_directories.extend(
                [
                    "/usr/local/bin",
                    "/opt/homebrew/bin",
                    "/usr/bin",
                    "/bin",
                    "/usr/sbin",
                    "/sbin",
                ]
                .map(PathBuf::from),
            );
            if let Ok(versions) = fs::read_dir(home.join(".nvm/versions/node")) {
                command_directories
                    .extend(versions.flatten().map(|version| version.path().join("bin")));
            }
            if platform == HostPlatform::MacOs {
                application_directories
                    .extend([PathBuf::from("/Applications"), home.join("Applications")]);
            }
        }
        Self {
            platform,
            home,
            command_directories,
            application_directories,
            executable_extensions: std::env::var("PATHEXT")
                .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
                .split(';')
                .map(str::to_owned)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyConnectionSummary {
    pub state: &'static str,
    pub owner: Option<serde_json::Value>,
    pub bound_workspace: Option<serde_json::Value>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInventoryEntry {
    pub id: ToolId,
    pub name: &'static str,
    pub installed: bool,
    pub can_connect: bool,
    pub download_url: &'static str,
    pub config_path: String,
    pub connection: Option<LegacyConnectionSummary>,
}

pub struct ToolInventory {
    environment: InstallationEnvironment,
}
impl ToolInventory {
    pub fn new(environment: InstallationEnvironment) -> Self {
        Self { environment }
    }
    pub fn current() -> Self {
        Self::new(InstallationEnvironment::current())
    }
    pub fn list(&self) -> Result<Vec<ToolInventoryEntry>> {
        let records = self.records()?;
        let desktop = crate::connections::ConnectionManager::new(
            self.environment
                .home
                .join(".tokensmarket/desktop-connections"),
            String::new(),
            String::new(),
        )
        .list()?;
        Ok(adapters()
            .into_iter()
            .map(|adapter| {
                let installed = self.is_installed(adapter.id);
                ToolInventoryEntry {
                    id: adapter.id,
                    name: adapter.name,
                    installed,
                    can_connect: installed,
                    download_url: official_download_url(adapter.id),
                    config_path: adapter.config_path().display().to_string(),
                    connection: desktop
                        .iter()
                        .find(|r| r.tool == adapter.id)
                        .map(|r| LegacyConnectionSummary {
                            state: "configured_untested",
                            owner: Some(serde_json::json!(r.owner)),
                            bound_workspace: Some(serde_json::json!(r.bound_workspace)),
                            models: r.models.clone(),
                        })
                        .or_else(|| {
                            records.iter().find(|r| r.tool == adapter.id).map(|r| {
                                LegacyConnectionSummary {
                                    state: "legacy",
                                    owner: None,
                                    bound_workspace: None,
                                    models: vec![r.model.clone()],
                                }
                            })
                        }),
                }
            })
            .collect())
    }
    /// Rechecks the filesystem immediately before a connection operation begins.
    pub fn require_installed(&self, tool: ToolId) -> Result<()> {
        if !self.is_installed(tool) {
            bail!(
                "{} 未安装，请先从官网下载并重新检测",
                get_adapter(tool).name
            );
        }
        Ok(())
    }
    pub fn is_installed(&self, tool: ToolId) -> bool {
        let commands: &[&str] = match tool {
            ToolId::ClaudeCode => &["claude"],
            ToolId::Codex => &["codex"],
            ToolId::WorkBuddy => &["workbuddy"],
            ToolId::DeepSeekHarness => &["dsh"],
            ToolId::OpenClaw => &["openclaw"],
            ToolId::OpenCode => &["opencode"],
        };
        commands
            .iter()
            .any(|command| self.environment.has_command(command))
            || (matches!(
                tool,
                ToolId::WorkBuddy | ToolId::DeepSeekHarness | ToolId::OpenClaw | ToolId::OpenCode
            ) && self.environment.platform == HostPlatform::MacOs
                && self
                    .environment
                    .application_directories
                    .iter()
                    .any(|directory| {
                        macos_bundle_installed(
                            &directory.join(format!("{}.app", get_adapter(tool).name)),
                        )
                    }))
            || (self.environment.platform == HostPlatform::Windows
                && self
                    .environment
                    .application_directories
                    .iter()
                    .any(|directory| windows_desktop_installed(directory, tool)))
    }
    /// Verified desktop application location for the Harness launch action.
    pub fn harness_application_path(&self) -> Option<PathBuf> {
        for directory in &self.environment.application_directories {
            if self.environment.platform == HostPlatform::MacOs {
                let bundle = directory.join("DeepSeek Harness.app");
                if macos_bundle_installed(&bundle) {
                    return Some(bundle);
                }
            } else if self.environment.platform == HostPlatform::Windows {
                for folder in [
                    directory.clone(),
                    directory.join("DeepSeek Harness"),
                    directory.join("deepseek-harness"),
                ] {
                    if let Ok(entries) = fs::read_dir(folder) {
                        for entry in entries.flatten() {
                            if ["DeepSeek Harness.exe", "deepseek-harness.exe"]
                                .iter()
                                .any(|name| {
                                    entry
                                        .file_name()
                                        .to_string_lossy()
                                        .eq_ignore_ascii_case(name)
                                })
                                && is_executable(&entry.path(), HostPlatform::Windows)
                            {
                                return Some(entry.path());
                            }
                        }
                    }
                }
            }
        }
        None
    }
    fn records(&self) -> Result<Vec<ManagedKeyRecord>> {
        let path = self.environment.home.join(".tokensmarket/keys.json");
        match fs::read_to_string(&path) {
            Ok(text) if text.trim().is_empty() => Ok(vec![]),
            Ok(text) => serde_json::from_str(&text).context("读取已有工具连接记录失败"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e).context("读取已有工具连接记录失败"),
        }
    }
}

pub fn official_download_url(tool: ToolId) -> &'static str {
    match tool {
        ToolId::ClaudeCode => "https://code.claude.com/docs/en/setup",
        ToolId::Codex => "https://developers.openai.com/codex/cli/",
        ToolId::WorkBuddy => "https://www.workbuddy.ai/",
        ToolId::DeepSeekHarness => "https://www.deepseek.com/download/",
        ToolId::OpenClaw => "https://docs.openclaw.ai/install",
        ToolId::OpenCode => "https://opencode.ai/docs/",
    }
}

impl InstallationEnvironment {
    pub(crate) fn has_command(&self, command: &str) -> bool {
        self.command_directories.iter().any(|directory| {
            if self.platform != HostPlatform::Windows {
                return is_executable(&directory.join(command), self.platform);
            }
            self.executable_extensions
                .iter()
                .filter(|ext| {
                    [".exe", ".com", ".bat", ".cmd"].contains(&ext.to_ascii_lowercase().as_str())
                })
                .any(|ext| windows_file(directory, &format!("{command}{ext}")))
        })
    }
}

fn is_executable(path: &Path, platform: HostPlatform) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    if platform == HostPlatform::Windows {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn macos_bundle_installed(path: &Path) -> bool {
    let Ok(info) = plist::Value::from_file(path.join("Contents/Info.plist")) else {
        return false;
    };
    let Some(executable) = info
        .as_dictionary()
        .and_then(|d| d.get("CFBundleExecutable"))
        .and_then(plist::Value::as_string)
    else {
        return false;
    };
    // CFBundleExecutable is a filename, not an arbitrary relative/absolute path.
    if Path::new(executable).components().count() != 1 || executable == "." || executable == ".." {
        return false;
    }
    is_executable(
        &path.join("Contents/MacOS").join(executable),
        HostPlatform::MacOs,
    )
}

fn windows_file(directory: &Path, file: &str) -> bool {
    fs::read_dir(directory).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(file)
                && is_executable(&entry.path(), HostPlatform::Windows)
        })
    })
}

fn windows_desktop_installed(directory: &Path, tool: ToolId) -> bool {
    let (folders, files): (&[&str], &[&str]) = match tool {
        ToolId::WorkBuddy => (&["WorkBuddy"], &["WorkBuddy.exe"]),
        ToolId::DeepSeekHarness => (
            &["DeepSeek Harness", "deepseek-harness"],
            &["DeepSeek Harness.exe", "deepseek-harness.exe"],
        ),
        ToolId::OpenClaw => (
            &["OpenClaw", "OpenClaw Companion"],
            &["OpenClaw.exe", "OpenClawCompanion.exe"],
        ),
        ToolId::OpenCode => (&["OpenCode", "opencode"], &["OpenCode.exe", "opencode.exe"]),
        _ => return false,
    };
    files.iter().any(|file| {
        windows_file(directory, file)
            || folders
                .iter()
                .any(|folder| windows_file(&directory.join(folder), file))
    })
}

#[cfg(windows)]
fn registered_application_directories() -> Vec<PathBuf> {
    use winreg::{
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
        RegKey,
    };
    let mut paths = vec![];
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
            let Ok(uninstall) = RegKey::predef(hive).open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
                KEY_READ | view,
            ) else {
                continue;
            };
            for key in uninstall.enum_keys().flatten() {
                let Ok(app) = uninstall.open_subkey_with_flags(key, KEY_READ | view) else {
                    continue;
                };
                let Ok(name) = app.get_value::<String, _>("DisplayName") else {
                    continue;
                };
                if !["workbuddy", "deepseek harness", "opencode", "openclaw"]
                    .iter()
                    .any(|product| name.to_ascii_lowercase().contains(product))
                {
                    continue;
                }
                if let Ok(location) = app.get_value::<String, _>("InstallLocation") {
                    if !location.trim().is_empty() {
                        paths.push(PathBuf::from(location));
                    }
                }
            }
        }
    }
    paths
}

#[cfg(windows)]
fn registered_command_directories() -> Vec<PathBuf> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
        RegKey,
    };
    let mut paths = vec![];
    for (hive, key) in [
        (HKEY_CURRENT_USER, r"Environment"),
        (
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        ),
    ] {
        let Ok(environment) = RegKey::predef(hive).open_subkey(key) else {
            continue;
        };
        let Ok(path) = environment.get_value::<String, _>("Path") else {
            continue;
        };
        for value in path.split(';').filter(|value| !value.trim().is_empty()) {
            // Registry PATH may be REG_EXPAND_SZ. Only expand environment
            // variables, never invoke a shell or launcher during discovery.
            let mut expanded = String::new();
            let mut rest = value.trim().trim_matches('"');
            while let Some(start) = rest.find('%') {
                expanded.push_str(&rest[..start]);
                let tail = &rest[start + 1..];
                let Some(end) = tail.find('%') else {
                    expanded.push_str(&rest[start..]);
                    rest = "";
                    break;
                };
                let variable = &tail[..end];
                expanded
                    .push_str(&std::env::var(variable).unwrap_or_else(|_| format!("%{variable}%")));
                rest = &tail[end + 1..];
            }
            expanded.push_str(rest);
            paths.push(PathBuf::from(expanded));
        }
    }
    paths
}

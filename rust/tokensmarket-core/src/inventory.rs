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
    pub key_id: Option<String>,
    pub protocol: Option<String>,
    pub test_result: Option<crate::connections::ConnectionTestResult>,
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
        .summaries()?;
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
                    config_path: self.config_path(adapter.id).display().to_string(),
                    connection: desktop
                        .iter()
                        .find(|(tool, _)| *tool == adapter.id)
                        .map(|(_, r)| LegacyConnectionSummary {
                            state: r.state,
                            owner: Some(r.owner.clone()),
                            bound_workspace: Some(r.bound_workspace.clone()),
                            models: r.models.clone(),
                            key_id: Some(r.key_id.clone()),
                            protocol: Some(r.protocol.clone()),
                            test_result: r.test_result.clone(),
                        })
                        .or_else(|| {
                            records.iter().find(|r| r.tool == adapter.id).map(|r| {
                                LegacyConnectionSummary {
                                    state: "legacy",
                                    owner: None,
                                    bound_workspace: None,
                                    models: vec![r.model.clone()],
                                    key_id: None,
                                    protocol: None,
                                    test_result: None,
                                }
                            })
                        }),
                }
            })
            .collect())
    }
    /// Desktop discovery and desktop writes share this path. Terminal CLI paths are unchanged.
    pub fn config_path(&self, tool: ToolId) -> PathBuf {
        if tool == ToolId::WorkBuddy {
            let override_path = |name: &str| {
                std::env::var_os(name)
                    .filter(|v| !v.to_string_lossy().trim().is_empty())
                    .map(PathBuf::from)
            };
            return override_path("WORKBUDDY_CONFIG_PATH").unwrap_or_else(|| {
                override_path("WORKBUDDY_CONFIG_DIR")
                    .or_else(|| override_path("CODEBUDDY_CONFIG_DIR"))
                    .unwrap_or_else(|| self.environment.home.join(".workbuddy"))
                    .join("models.json")
            });
        }
        if tool == ToolId::OpenCode {
            if let Some(path) = std::env::var_os("OPENCODE_CONFIG")
                .filter(|v| !v.to_string_lossy().trim().is_empty())
            {
                return path.into();
            }
            let base = self.environment.home.join(".config/opencode");
            return if base.join("opencode.jsonc").exists() {
                base.join("opencode.jsonc")
            } else {
                base.join("opencode.json")
            };
        }
        get_adapter(tool).config_path()
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
            || self.application_path(tool).is_some()
    }
    /// Verified application launch target; command launchers are handled separately.
    pub fn application_path(&self, tool: ToolId) -> Option<PathBuf> {
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
            _ => return None,
        };
        for directory in &self.environment.application_directories {
            if self.environment.platform == HostPlatform::MacOs {
                for name in folders {
                    let bundle = directory.join(format!("{name}.app"));
                    if macos_bundle_installed(&bundle) {
                        return Some(bundle);
                    }
                }
            } else if self.environment.platform == HostPlatform::Windows {
                for folder in std::iter::once(directory.clone())
                    .chain(folders.iter().map(|name| directory.join(name)))
                {
                    if let Ok(entries) = fs::read_dir(folder) {
                        for entry in entries.flatten() {
                            if files.iter().any(|name| {
                                entry
                                    .file_name()
                                    .to_string_lossy()
                                    .eq_ignore_ascii_case(name)
                            }) && is_executable(&entry.path(), HostPlatform::Windows)
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
    pub fn harness_application_path(&self) -> Option<PathBuf> {
        self.application_path(ToolId::DeepSeekHarness)
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
        self.command_path(command).is_some()
    }
    fn command_path(&self, command: &str) -> Option<PathBuf> {
        for directory in &self.command_directories {
            if self.platform != HostPlatform::Windows {
                let path = directory.join(command);
                if is_executable(&path, self.platform) {
                    return Some(path);
                }
            } else {
                for ext in &self.executable_extensions {
                    if ![".exe", ".com", ".bat", ".cmd"]
                        .contains(&ext.to_ascii_lowercase().as_str())
                    {
                        continue;
                    }
                    if let Ok(entries) = fs::read_dir(directory) {
                        for entry in entries.flatten() {
                            if entry
                                .file_name()
                                .to_string_lossy()
                                .eq_ignore_ascii_case(&format!("{command}{ext}"))
                                && is_executable(&entry.path(), self.platform)
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
        // Non-Unix hosts have no POSIX execute bit. The current native Windows
        // inventory already uses its Windows branch above; allow declared-platform
        // filesystem fixtures to model macOS command presence on that host too.
        true
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

/// An inspectable OS command plan. Creating it only discovers files; it never launches a process.
#[derive(Debug)]
pub struct ToolLaunchPlan {
    pub program: String,
    pub args: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub wait_for_exit: bool,
}
impl ToolInventory {
    /// Resolve against this fresh installation snapshot, including registry PATH and nvm.
    pub fn launch_plan(&self, tool: ToolId) -> Result<ToolLaunchPlan> {
        self.require_installed(tool)?;
        if let Some(path) = self.application_path(tool) {
            return match self.environment.platform {
                HostPlatform::MacOs => Ok(ToolLaunchPlan {
                    program: "open".into(),
                    args: vec!["-a".into(), path.to_string_lossy().into()],
                    environment: vec![],
                    wait_for_exit: true,
                }),
                HostPlatform::Windows => Ok(ToolLaunchPlan {
                    program: path.to_string_lossy().into(),
                    args: vec![],
                    environment: vec![],
                    wait_for_exit: false,
                }),
                HostPlatform::Unix => bail!("请从系统应用入口打开工具"),
            };
        }
        let command = match tool {
            ToolId::ClaudeCode => "claude",
            ToolId::Codex => "codex",
            ToolId::DeepSeekHarness => "dsh",
            ToolId::WorkBuddy => "workbuddy",
            ToolId::OpenClaw => "openclaw",
            ToolId::OpenCode => "opencode",
        };
        let target = self
            .environment
            .command_path(command)
            .context("工具启动文件已移除，请重新检测")?;
        let mut directories = vec![target.parent().context("工具启动目录无效")?.to_path_buf()];
        directories.extend(
            self.environment
                .command_directories
                .iter()
                .filter(|p| !directories.contains(p))
                .cloned()
                .collect::<Vec<_>>(),
        );
        let delimiter = if self.environment.platform == HostPlatform::Windows {
            ";"
        } else {
            ":"
        };
        let path = directories
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join(delimiter);
        if self.environment.platform == HostPlatform::Windows {
            // Only fixed command names enter cmd text. Discovered paths stay in the child
            // environment, so spaces, ampersands and .cmd launchers need no shell interpolation.
            let fixed = if tool == ToolId::OpenClaw {
                "openclaw tui"
            } else {
                command
            };
            return Ok(ToolLaunchPlan {
                program: "cmd".into(),
                args: ["/c", "start", "", "cmd", "/k", fixed]
                    .map(str::to_owned)
                    .to_vec(),
                environment: vec![("PATH".into(), path)],
                wait_for_exit: false,
            });
        }
        if self.environment.platform == HostPlatform::MacOs {
            fn shell_quote(value: &str) -> String {
                format!("'{}'", value.replace('\'', "'\"'\"'"))
            }
            let terminal = format!(
                "env PATH={} {}{}",
                shell_quote(&path),
                shell_quote(&target.to_string_lossy()),
                if tool == ToolId::OpenClaw { " tui" } else { "" }
            );
            let script = terminal.replace('\\', "\\\\").replace('"', "\\\"");
            return Ok(ToolLaunchPlan {
                program: "osascript".into(),
                args: vec![
                    "-e".into(),
                    "tell application \"Terminal\" to activate".into(),
                    "-e".into(),
                    format!("tell application \"Terminal\" to do script \"{script}\""),
                ],
                environment: vec![],
                wait_for_exit: true,
            });
        }
        bail!("请打开终端运行工具")
    }
}

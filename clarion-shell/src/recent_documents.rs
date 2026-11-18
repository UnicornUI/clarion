//! Recently opened documents and projects for one Dock application.
//!
//! The only cross-application standard here is the freedesktop [Recent File
//! Storage] specification: GLib's `GtkRecentManager` records every opened URI
//! in `$XDG_DATA_HOME/recently-used.xbel`, together with the applications that
//! opened it. The file has no per-application index, so we read the bookmarks,
//! keep the ones this application recorded and order them by the timestamps
//! the manager wrote.
//!
//! Editors that keep their own history never touch that file. Two on-disk
//! formats are read as well:
//!
//! * the VS Code family keeps one `User/workspaceStorage/<hash>/workspace.json`
//!   per opened folder or workspace, ordered by that file's modification time;
//! * JetBrains IDEs keep `options/recentProjects.xml` in their config
//!   directory, with one timestamped `<entry key="$USER_HOME$/…">` per project.
//!   The config directory is either `~/.config/JetBrains/<Product><version>`
//!   (or the legacy `~/.<Product><version>/config`) or, for self-contained
//!   installs, whatever `bin/idea.properties` names in `idea.config.path`.
//!
//! KDE's `RecentDocuments` files are global rather than per-application, so
//! they are not read here.
//!
//! [Recent File Storage]: https://www.freedesktop.org/wiki/Specifications/recent-file-spec/

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use clarion_launcher::catalog::DesktopApp;

/// How many entries the Dock click menu shows for one application.
pub(crate) const MAX_RECENT_DOCUMENTS: usize = 6;

/// One file or folder the application opened before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecentDocument {
    pub path: PathBuf,
    /// The file or folder name, painted first in the menu row.
    pub name: String,
    /// The parent folder with the home directory shortened to `~`.
    pub parent: String,
}

/// The application's recent documents and project folders, newest first.
pub(crate) fn for_app(
    app: &DesktopApp,
    environment: &HashMap<String, String>,
) -> Vec<RecentDocument> {
    let Some(home) = environment.get("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let data_home = environment
        .get("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("share"));
    let config_home = environment
        .get("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let desktop = std::fs::read_to_string(&app.desktop_file)
        .ok()
        .map(|text| desktop_entry(&text))
        .unwrap_or_default();
    let identity = Identity::for_app(app, &desktop);
    let mut found = xbel_documents(&identity, &data_home, &home);
    found.extend(code_workspaces(&identity, &config_home, &home));
    found.extend(jetbrains_projects(&desktop, &config_home, &home));
    // Newest first; the path breaks ties so the menu order is stable when the
    // manager wrote two entries in the same millisecond.
    found.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.path.cmp(&b.path)));
    let mut seen = HashSet::new();
    let mut documents = Vec::new();
    for entry in found {
        if !seen.insert(entry.path.clone()) {
            continue;
        }
        documents.push(entry.document);
        if documents.len() == MAX_RECENT_DOCUMENTS {
            break;
        }
    }
    documents
}

struct Found {
    path: PathBuf,
    at: i64,
    document: RecentDocument,
}

impl Found {
    fn new(path: PathBuf, at: i64, home: &Path) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| shorten(&path, home));
        let parent = path
            .parent()
            .map(|parent| shorten(parent, home))
            .unwrap_or_default();
        Self {
            document: RecentDocument { path: path.clone(), name, parent },
            path,
            at,
        }
    }
}

/// The identity strings one application answers to when the recent list names
/// it. GtkRecentManager records the executable's name for most applications,
/// the GApplication id for some and the human application name for others, so
/// all of them have to match. `strong` marks the names that actually identify
/// the program (desktop id, Exec, WM class, Flatpak id); the display name is
/// weak and may only match at a word boundary, never as a suffix.
struct Identity {
    candidates: Vec<(String, bool)>,
}

impl Identity {
    fn for_app(app: &DesktopApp, entry: &HashMap<String, String>) -> Self {
        let mut candidates = Vec::new();
        let add = |value: &str, strong: bool, candidates: &mut Vec<(String, bool)>| {
            for identity in identities(value, false) {
                push_candidate(candidates, identity, strong);
            }
        };
        add(&app.id, true, &mut candidates);
        add(&app.name, false, &mut candidates);
        if let Some(stem) = app.id.strip_suffix(".desktop") {
            add(stem, true, &mut candidates);
        }
        if let Some(stem) = app.desktop_file.file_stem() {
            add(&stem.to_string_lossy(), true, &mut candidates);
        }
        for (key, strong) in [
            ("Name", false),
            ("Exec", true),
            ("StartupWMClass", true),
            ("X-Flatpak", true),
            ("X-Flatpak-RenamedFrom", true),
        ] {
            let Some(value) = entry.get(key) else {
                continue;
            };
            for identity in identities(value, key == "Exec") {
                push_candidate(&mut candidates, identity, strong);
            }
        }
        Self { candidates }
    }

    /// Whether one recorded `application name=… exec=…` pair names this app.
    ///
    /// The recorded command line is the better witness: it names the program
    /// that actually opened the file. The human name is only used when the
    /// command is a generic opener (`gio open`, a desktop portal), because
    /// otherwise the portal writes the *initiating* application's name beside
    /// another program's command.
    fn matches(&self, name: &str, exec: &str) -> bool {
        let mut recorded = identities(exec, true);
        if recorded.is_empty() {
            recorded = identities(name, false);
        }
        self.candidates.iter().any(|(candidate, strong)| {
            recorded
                .iter()
                .any(|recorded| same_app(candidate, recorded, *strong))
        })
    }

    /// Whether one of this application's identifiers names `key`.
    fn matches_key(&self, key: &str) -> bool {
        let key = normalize(key);
        self.candidates
            .iter()
            .any(|(candidate, _)| same_app(candidate, &key, true))
    }
}

fn push_candidate(candidates: &mut Vec<(String, bool)>, value: String, strong: bool) {
    if value.len() < 2 {
        return;
    }
    if let Some((_, known_strong)) = candidates.iter_mut().find(|(known, _)| *known == value) {
        *known_strong |= strong;
    } else {
        candidates.push((value, strong));
    }
}

/// One bookmark in `recently-used.xbel`.
struct Bookmark {
    href: String,
    added: String,
    modified: String,
    visited: String,
    applications: Vec<RecordedApplication>,
}

struct RecordedApplication {
    name: String,
    exec: String,
    modified: String,
}

/// Every bookmark recorded by this application, newest first per entry.
fn xbel_documents(identity: &Identity, data_home: &Path, home: &Path) -> Vec<Found> {
    let Ok(text) = std::fs::read_to_string(data_home.join("recently-used.xbel")) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for bookmark in parse_xbel(&text) {
        let Some(path) = file_uri_path(&bookmark.href) else {
            continue;
        };
        if !path.is_absolute() || !path.exists() {
            continue;
        }
        let mut at = 0;
        let mut matched = false;
        for application in &bookmark.applications {
            if identity.matches(&application.name, &application.exec) {
                matched = true;
                at = at.max(timestamp(&application.modified));
            }
        }
        if !matched {
            continue;
        }
        at = at
            .max(timestamp(&bookmark.modified))
            .max(timestamp(&bookmark.visited))
            .max(timestamp(&bookmark.added));
        found.push(Found::new(path, at, home));
    }
    found
}

/// The VS Code family keeps one `workspaceStorage/<hash>/workspace.json` per
/// opened folder or workspace. The config directory names the product, and the
/// identifiers accept the editor's usual executable and desktop ids.
const CODE_FAMILIES: &[(&str, &[&str])] = &[
    (
        "Code",
        &["com.microsoft.vscode", "visual studio code", "code"],
    ),
    (
        "Code - Insiders",
        &["com.microsoft.vscodeinsiders", "code - insiders", "code-insiders"],
    ),
    (
        "VSCodium",
        &["com.vscodium.codium", "visual studio codium", "codium", "vscodium"],
    ),
    ("Cursor", &["cursor"]),
    ("Windsurf", &["windsurf"]),
];

fn code_workspaces(identity: &Identity, config_home: &Path, home: &Path) -> Vec<Found> {
    let mut found = Vec::new();
    for (directory, keys) in CODE_FAMILIES {
        if !keys.iter().any(|key| identity.matches_key(key)) {
            continue;
        }
        let storage = config_home
            .join(directory)
            .join("User")
            .join("workspaceStorage");
        let Ok(entries) = std::fs::read_dir(storage) else {
            continue;
        };
        for entry in entries.flatten() {
            let workspace = entry.path().join("workspace.json");
            let Ok(metadata) = std::fs::metadata(&workspace) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&workspace) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let uri = value
                .get("folder")
                .or_else(|| value.get("workspace"))
                .and_then(|value| value.as_str());
            let Some(path) = uri.and_then(file_uri_path) else {
                continue;
            };
            if !path.exists() {
                continue;
            }
            let at = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_millis() as i64);
            found.push(Found::new(path, at, home));
        }
    }
    found
}

/// The JetBrains IDE product one desktop entry launches. The launcher scripts
/// are named after the product regardless of edition, so the family carries
/// every directory prefix JetBrains uses for that product.
struct JetBrainsFamily {
    prefixes: &'static [&'static str],
    /// The XDG config roots that hold the product directories.
    roots: &'static [&'static str],
}

fn jetbrains_family(entry: &HashMap<String, String>) -> Option<&'static JetBrainsFamily> {
    const JETBRAINS: &[&str] = &["JetBrains"];
    const JETBRAINS_GOOGLE: &[&str] = &["Google"];
    const IDEA: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["IntelliJIdea", "IdeaIC"],
        roots: JETBRAINS,
    };
    const PYCHARM: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["PyCharm"],
        roots: JETBRAINS,
    };
    const CLION: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["CLion"],
        roots: JETBRAINS,
    };
    const GOLAND: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["GoLand"],
        roots: JETBRAINS,
    };
    const WEBSTORM: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["WebStorm"],
        roots: JETBRAINS,
    };
    const PHPSTORM: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["PhpStorm"],
        roots: JETBRAINS,
    };
    const RUBYMINE: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["RubyMine"],
        roots: JETBRAINS,
    };
    const DATAGRIP: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["DataGrip"],
        roots: JETBRAINS,
    };
    const RIDER: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["Rider"],
        roots: JETBRAINS,
    };
    const RUSTROVER: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["RustRover"],
        roots: JETBRAINS,
    };
    const AQUA: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["Aqua"],
        roots: JETBRAINS,
    };
    const DATASPELL: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["DataSpell"],
        roots: JETBRAINS,
    };
    const ANDROID_STUDIO: JetBrainsFamily = JetBrainsFamily {
        prefixes: &["AndroidStudio"],
        roots: JETBRAINS_GOOGLE,
    };
    let exec = entry.get("Exec")?;
    let program = shell_words(exec).into_iter().next()?;
    let name = Path::new(&program)
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let name = name.strip_suffix(".sh").unwrap_or(&name);
    let name = name.strip_suffix(".exe").unwrap_or(name);
    Some(match name {
        "idea" | "idea64" => &IDEA,
        "pycharm" | "pycharm64" => &PYCHARM,
        "clion" | "clion64" => &CLION,
        "goland" | "goland64" => &GOLAND,
        "webstorm" | "webstorm64" => &WEBSTORM,
        "phpstorm" | "phpstorm64" => &PHPSTORM,
        "rubymine" | "rubymine64" => &RUBYMINE,
        "datagrip" | "datagrip64" => &DATAGRIP,
        "rider" | "rider64" => &RIDER,
        "rustrover" | "rustrover64" => &RUSTROVER,
        "aqua" | "aqua64" => &AQUA,
        "dataspell" | "dataspell64" => &DATASPELL,
        "studio" | "studio64" | "android-studio" => &ANDROID_STUDIO,
        _ => return None,
    })
}

/// Recent projects from a JetBrains IDE. Two layouts exist:
///
/// * the standard config directory (`~/.config/JetBrains/<Product><version>`,
///   Toolbox and tarball installs, or the legacy `~/.<Product><version>/config`
///   for old versions), and
/// * a self-contained install whose `bin/idea.properties` points
///   `idea.config.path` inside the installation (`${idea.home.path}/runtime/config`).
///
/// Both hold `options/recentProjects.xml`; when several product versions are
/// present the newest file is the one the user actually writes.
fn jetbrains_projects(
    entry: &HashMap<String, String>,
    config_home: &Path,
    home: &Path,
) -> Vec<Found> {
    let Some(family) = jetbrains_family(entry) else {
        return Vec::new();
    };
    let install_root = entry
        .get("Exec")
        .and_then(|exec| shell_words(exec).into_iter().next())
        .and_then(|program| {
            // Toolbox and distribution packages often put a symlink in
            // /usr/bin; the properties live beside the real binary.
            let program = std::fs::canonicalize(&program).unwrap_or_else(|_| PathBuf::from(program));
            let bin = program.parent()?;
            if bin.file_name()? != "bin" {
                return None;
            }
            bin.parent().map(Path::to_path_buf)
        });
    let mut configs = Vec::new();
    if let Some(root) = install_root.as_deref() {
        if let Some(config) = jetbrains_self_contained_config(root, home) {
            configs.push(config);
        }
    }
    for root in family.roots {
        let Ok(entries) = std::fs::read_dir(config_home.join(root)) else {
            continue;
        };
        for entry in entries.flatten() {
            if family
                .prefixes
                .iter()
                .any(|prefix| entry.file_name().to_string_lossy().starts_with(prefix))
            {
                configs.push(entry.path());
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(home) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if family
                .prefixes
                .iter()
                .any(|prefix| name.starts_with(&format!(".{prefix}")))
            {
                configs.push(entry.path().join("config"));
            }
        }
    }
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for config in configs {
        let file = config.join("options").join("recentProjects.xml");
        let Ok(metadata) = std::fs::metadata(&file) else {
            continue;
        };
        let modified = metadata
            .modified()
            .unwrap_or(std::time::UNIX_EPOCH);
        if newest
            .as_ref()
            .is_none_or(|(best, _)| modified > *best)
        {
            newest = Some((modified, file));
        }
    }
    let Some((_, file)) = newest else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    parse_recent_projects(&text, home, install_root.as_deref())
}

/// `idea.properties` may point the config away from the home directory, which
/// is how these installs keep everything inside their own tree.
fn jetbrains_self_contained_config(root: &Path, home: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(root.join("bin").join("idea.properties")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "idea.config.path" {
            continue;
        }
        let value = resolve_macros(value.trim(), home, Some(root))?;
        let path = PathBuf::from(value);
        if path.as_os_str().is_empty() {
            continue;
        }
        return Some(if path.is_absolute() { path } else { root.join(path) });
    }
    None
}

/// `RecentProjectsManager` stores one `<entry key="$USER_HOME$/…">` per project
/// with an `activationTimestamp` in milliseconds. Older versions used a bare
/// `recentPaths` list without timestamps; that list is still read.
fn parse_recent_projects(text: &str, home: &Path, install_root: Option<&Path>) -> Vec<Found> {
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("<entry ") {
        let start = cursor + offset;
        let Some(tag_end) = text[start..].find('>').map(|end| start + end) else {
            break;
        };
        let attributes = attributes(&text[start + "<entry".len()..tag_end]);
        let Some(key) = attributes.get("key").cloned() else {
            cursor = tag_end;
            continue;
        };
        let body_start = tag_end + 1;
        let body_end = text[body_start..]
            .find("</entry>")
            .map(|end| body_start + end)
            .unwrap_or(text.len());
        let body = &text[body_start..body_end];
        let at = option_timestamp(body, "activationTimestamp")
            .max(option_timestamp(body, "projectOpenTimestamp"));
        if let Some(path) = project_path(&key, home, install_root) {
            if path.exists() {
                found.push(Found::new(path, at, home));
            }
        }
        cursor = body_end;
    }
    if found.is_empty() {
        for key in recent_paths_list(text) {
            if let Some(path) = project_path(&key, home, install_root) {
                if path.exists() {
                    found.push(Found::new(path, 0, home));
                }
            }
        }
    }
    found
}

fn option_timestamp(body: &str, name: &str) -> i64 {
    let needle = format!("name=\"{name}\"");
    let Some(offset) = body.find(&needle) else {
        return 0;
    };
    let tag_start = body[..offset].rfind('<').unwrap_or(0);
    let Some(tag_end) = body[offset..].find('>').map(|end| offset + end) else {
        return 0;
    };
    attributes(&body[tag_start..tag_end])
        .get("value")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn recent_paths_list(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let Some(offset) = text.find("name=\"recentPaths\"") else {
        return paths;
    };
    let Some(list) = text[offset..].find("<list>").map(|start| offset + start) else {
        return paths;
    };
    let Some(end) = text[list..].find("</list>").map(|end| list + end) else {
        return paths;
    };
    let block = &text[list..end];
    let mut cursor = 0;
    while let Some(offset) = block[cursor..].find("<option value=") {
        let start = cursor + offset;
        let Some(tag_end) = block[start..].find('>').map(|end| start + end) else {
            break;
        };
        let attributes = attributes(&block[start + "<option".len()..tag_end]);
        if let Some(value) = attributes.get("value") {
            paths.push(value.clone());
        }
        cursor = tag_end;
    }
    paths
}

fn project_path(key: &str, home: &Path, install_root: Option<&Path>) -> Option<PathBuf> {
    let value = resolve_macros(key, home, install_root)?;
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

/// Resolve the path macros both the recent list and `idea.properties` use.
fn resolve_macros(value: &str, home: &Path, install: Option<&Path>) -> Option<String> {
    let mut value = value.trim().to_owned();
    if value.is_empty() {
        return None;
    }
    let home = home.to_string_lossy().into_owned();
    value = value.replace("$USER_HOME$", &home);
    value = value.replace("${user.home}", &home);
    if value.contains("$APPLICATION_HOME_DIR$") || value.contains("${idea.home.path}") {
        let install = install?;
        let install = install.to_string_lossy().into_owned();
        value = value.replace("$APPLICATION_HOME_DIR$", &install);
        value = value.replace("${idea.home.path}", &install);
    }
    if let Some(rest) = value.strip_prefix("~/").map(str::to_owned) {
        value = format!("{home}/{rest}");
    } else if value == "~" {
        value = home;
    }
    Some(value)
}

/// The identifiers one desktop-entry value can be recorded under: the whole
/// value, the executable basenames, and reverse-DNS ids embedded in a Flatpak
/// command line.
/// The identifiers one desktop-entry or recent-list value records.
///
/// A plain field (a name, WM class or Flatpak id) is an identifier as a whole;
/// splitting it would turn `GNOME Network Displays` into the generic words
/// `gnome` and `displays`, which then match unrelated applications. An `exec`
/// value is a command line, so only its program and embedded reverse-DNS ids
/// count; wrapper programs (`flatpak`, shell, portals, `env`, the Electron
/// runtime) are skipped so they do not become shared identities.
fn identities(value: &str, exec: bool) -> Vec<String> {
    let mut out = Vec::new();
    let value = strip_outer_quotes(value.trim());
    if value.is_empty() {
        return out;
    }
    if !exec {
        for part in value.split(';') {
            push_identity(&mut out, part);
        }
        return out;
    }
    let mut command_done = false;
    for token in shell_words(value) {
        if let Some(command) = token.strip_prefix("--command=") {
            push_identity(&mut out, command);
            continue;
        }
        if token.starts_with('-') || token.contains('%') || token.contains('=') {
            continue;
        }
        if is_reverse_dns(&token) {
            push_identity(&mut out, &token);
        }
        if command_done {
            continue;
        }
        let Some(name) = Path::new(&token).file_name() else {
            continue;
        };
        let name = normalize(&name.to_string_lossy());
        if name.is_empty() {
            continue;
        }
        if GENERIC_PROGRAMS.contains(&name.as_str()) {
            // `env`-style wrappers still name the real program next; `flatpak`
            // and interpreters do not, so the reverse-DNS ids stand alone.
            command_done = !EXEC_WRAPPERS.contains(&name.as_str());
            continue;
        }
        out.push(name);
        command_done = true;
    }
    out
}

/// The executable names that never identify an application by themselves.
const GENERIC_PROGRAMS: &[&str] = &[
    "gio",
    "xdg-open",
    "flatpak",
    "snap",
    "env",
    "sh",
    "bash",
    "dash",
    "zsh",
    "electron",
    "node",
    "python",
    "python3",
    "java",
    "mono",
    "wine",
    "bwrap",
    "systemd-run",
    "firejail",
    "gamemoderun",
    "prime-run",
    "nohup",
    "setsid",
    "xdg-desktop-portal",
    "xdg-desktop-portal-gtk",
    "xdg-desktop-portal-gnome",
    "xdg-desktop-portal-kde",
    "kde-open",
    "kde-open5",
    "exo-open",
    "kioclient5",
];

/// Wrappers whose next positional word is still the real program.
const EXEC_WRAPPERS: &[&str] = &["env", "nohup", "setsid", "gamemoderun", "prime-run"];

/// A whole-value quote pair is GLib's shell quoting of a command line, not an
/// argument to one.
fn strip_outer_quotes(value: &str) -> &str {
    for quote in ['\'', '"'] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.trim();
        }
    }
    value
}

/// A reverse-DNS id embedded in a command line, e.g. a Flatpak app id.
fn is_reverse_dns(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    if value.matches('.').count() < 2 || !first.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    std::iter::once(first).chain(parts).all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    })
}

fn push_identity(out: &mut Vec<String>, value: &str) {
    let normalized = normalize(value);
    if !normalized.is_empty() {
        out.push(normalized);
    }
}

/// Lowercase and drop the characters that are never part of an app identity,
/// keeping the separators so `zed` remains a word of `zed-editor`.
fn normalize(value: &str) -> String {
    value
        .trim()
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The identity with every separator removed, so `cosmic-files` and
/// `cosmicfiles` compare equal.
fn packed(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
}

fn prefixed(long: &str, short: &str) -> bool {
    long.strip_prefix(short).is_some_and(|rest| {
        rest.starts_with(['-', '.', '_', ' '])
    })
}

/// The mirror of [`prefixed`]: `party` names `clash-party`, but only an
/// identifier strong enough to be the application's own name may use it, so
/// the display name `Files` never matches another program's `cosmic-files`.
fn suffixed(long: &str, short: &str) -> bool {
    long.strip_suffix(short).is_some_and(|rest| {
        rest.is_empty() || rest.ends_with(['-', '.', '_', ' '])
    })
}

/// Whether two recorded identifiers name the same application. Equality and
/// separator-insensitive equality are strongest; a dotted identity also names
/// its own components (`org.gnome.Nautilus` ↔ `nautilus`); a word of a longer
/// name matches at a separator boundary (`zed` ↔ `zed-editor`).
fn same_app(a: &str, b: &str, allow_suffix: bool) -> bool {
    let a = normalize(a);
    let b = normalize(b);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b || packed(&a) == packed(&b) {
        return true;
    }
    if prefixed(&a, &b) || prefixed(&b, &a) {
        return true;
    }
    if allow_suffix && (suffixed(&a, &b) || suffixed(&b, &a)) {
        return true;
    }
    component_matches(&a, &b) || component_matches(&b, &a)
}

fn component_matches(dotted: &str, other: &str) -> bool {
    dotted
        .split('.')
        .map(normalize)
        .any(|part| part.chars().count() >= 4 && (part == other || packed(&part) == packed(other)))
}

/// A tiny shell-like splitter: the desktop entry and the recent list quote
/// arguments, and the field codes are separate words.
fn shell_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in value.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            c if c.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// The keys of the entry's base `[Desktop Entry]` group. Action groups carry
/// their own `Name`/`Exec` and must not be read as the application's.
fn desktop_entry(text: &str) -> HashMap<String, String> {
    let mut entry = HashMap::new();
    let mut in_base = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(group) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
            in_base = group.eq_ignore_ascii_case("Desktop Entry");
            continue;
        }
        if !in_base || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        // A localized key such as `Name[zh_CN]` must not shadow the base Name.
        if key.ends_with(']') {
            continue;
        }
        entry.entry(key.trim().to_owned()).or_insert(value.trim().to_owned());
    }
    entry
}

fn parse_xbel(text: &str) -> Vec<Bookmark> {
    let mut bookmarks = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("<bookmark ") {
        let start = cursor + offset;
        let Some(tag_end) = text[start..].find('>').map(|end| start + end) else {
            break;
        };
        let attributes = attributes(&text[start + "<bookmark ".len()..tag_end]);
        let body_start = tag_end + 1;
        let body_end = text[body_start..]
            .find("</bookmark>")
            .map(|end| body_start + end)
            .unwrap_or(text.len());
        bookmarks.push(Bookmark {
            href: attributes.get("href").cloned().unwrap_or_default(),
            added: attributes.get("added").cloned().unwrap_or_default(),
            modified: attributes.get("modified").cloned().unwrap_or_default(),
            visited: attributes.get("visited").cloned().unwrap_or_default(),
            applications: parse_applications(&text[body_start..body_end]),
        });
        cursor = body_end;
    }
    bookmarks
}

fn parse_applications(body: &str) -> Vec<RecordedApplication> {
    let mut applications = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = body[cursor..].find("<bookmark:application") {
        let start = cursor + offset;
        let Some(tag_end) = body[start..].find('>').map(|end| start + end) else {
            break;
        };
        let attributes = attributes(&body[start + "<bookmark:application".len()..tag_end]);
        applications.push(RecordedApplication {
            name: attributes.get("name").cloned().unwrap_or_default(),
            exec: attributes.get("exec").cloned().unwrap_or_default(),
            modified: attributes.get("modified").cloned().unwrap_or_default(),
        });
        cursor = tag_end;
    }
    applications
}

/// The attributes of one start tag. Values are entity-encoded, so a literal
/// `>` or quote never terminates the tag or the value early.
fn attributes(text: &str) -> HashMap<String, String> {
    let mut attributes = HashMap::new();
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find('=') {
        let equals = cursor + offset;
        let key = text[..equals]
            .trim_end()
            .rsplit(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut value_start = equals + 1;
        while matches!(bytes.get(value_start), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            value_start += 1;
        }
        let Some(quote) = text[value_start..]
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')
        else {
            cursor = equals + 1;
            continue;
        };
        let value_start = value_start + quote.len_utf8();
        let Some(end) = text[value_start..].find(quote) else {
            break;
        };
        attributes.insert(
            key,
            xml_unescape(&text[value_start..value_start + end]),
        );
        cursor = value_start + end + quote.len_utf8();
    }
    attributes
}

fn xml_unescape(value: &str) -> String {
    if !value.contains('&') {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(offset) = rest.find('&') {
        out.push_str(&rest[..offset]);
        let after = &rest[offset + 1..];
        let Some(end) = after.find(';') else {
            out.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..end];
        let replacement = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix('#')
                .and_then(|digits| {
                    if let Some(hex) = digits.strip_prefix(['x', 'X']) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        digits.parse().ok()
                    }
                })
                .and_then(char::from_u32),
        };
        match replacement {
            Some(character) => out.push(character),
            None => {
                out.push('&');
                out.push_str(entity);
                out.push(';');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn file_uri_path(href: &str) -> Option<PathBuf> {
    let rest = href.strip_prefix("file://")?;
    let rest = rest
        .strip_prefix("localhost")
        .filter(|rest| rest.starts_with('/'))
        .unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    Some(PathBuf::from(percent_decode(rest)))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if let (Some(high), Some(low)) = (bytes.get(index + 1), bytes.get(index + 2)) {
                if let (Some(high), Some(low)) = (hex_digit(*high), hex_digit(*low)) {
                    out.push(high * 16 + low);
                    index += 3;
                    continue;
                }
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn timestamp(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map_or(0, |timestamp| timestamp.timestamp_millis())
}

fn shorten(path: &Path, home: &Path) -> String {
    if home.as_os_str().is_empty() {
        return path.display().to_string();
    }
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, desktop: &Path) -> DesktopApp {
        DesktopApp {
            id: id.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            generic_name: String::new(),
            desktop_file: desktop.to_owned(),
            keywords: Vec::new(),
            categories: Vec::new(),
            category: clarion_launcher::catalog::AppCategory::Other,
            icon_path: None,
            icon_name: None,
            startup_wm_class: None,
            mime_types: Vec::new(),
        }
    }

    fn environment(home: &Path) -> HashMap<String, String> {
        [
            ("HOME".to_owned(), home.to_string_lossy().into_owned()),
            (
                "XDG_DATA_HOME".to_owned(),
                home.join("data").to_string_lossy().into_owned(),
            ),
            (
                "XDG_CONFIG_HOME".to_owned(),
                home.join("config").to_string_lossy().into_owned(),
            ),
        ]
        .into()
    }

    #[test]
    fn application_identities_match_the_names_glib_records() {
        let cases = [
            ("zed", "zed-editor", true),
            ("zed-editor", "zed", true),
            ("nautilus", "org.gnome.nautilus", true),
            ("org.gnome.nautilus", "nautilus", true),
            ("gimp", "gimp-2.10", true),
            ("pinta", "com.github.pintaproject.pinta", true),
            ("cosmic-files", "com.system76.cosmicfiles", true),
            ("code", "code", true),
            ("code", "codeblocks", false),
            ("clarion-shell", "gnome-shell", false),
            ("files", "org.gnome.nautilus", false),
            ("open", "org.gnome.nautilus", false),
        ];
        for (candidate, recorded, expected) in cases {
            assert_eq!(
                same_app(candidate, recorded, true),
                expected,
                "{candidate} vs {recorded}"
            );
        }
        // A suffix alias needs a strong identity: `party` names `clash-party`,
        // but the display names `Files`/`Displays` must not claim another
        // program's `cosmic-files`/`gnome-network-displays`.
        assert!(same_app("clash-party", "party", true));
        assert!(same_app("mihomo-party", "party", true));
        assert!(!same_app("files", "cosmic-files", false));
        assert!(!same_app("gnome network displays", "displays", false));
    }

    #[test]
    fn recorded_commands_identify_the_program_not_the_initiator() {
        // GLib quotes a whole command line; it is a command, not one word.
        assert_eq!(identities("'feh --start-at %u'", true), vec!["feh"]);
        assert_eq!(
            identities("'flatpak run --command=gnome-network-displays org.gnome.NetworkDisplays'", true),
            vec!["gnome-network-displays", "org.gnome.networkdisplays"]
        );
        // A human name is never split into generic words.
        assert_eq!(
            identities("GNOME Network Displays", false),
            vec!["gnome network displays"]
        );
        // The portal writes the initiating app's name beside the real command;
        // the command wins, and generic openers fall back to the name.
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let data = home.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let opened = home.join("report.pdf");
        std::fs::write(&opened, b"pdf").unwrap();
        let desktop = home.join("files.desktop");
        std::fs::write(
            &desktop,
            "[Desktop Entry]\nName=Files\nExec=nautilus --new-window %U\n",
        )
        .unwrap();
        let app = app("org.gnome.Nautilus.desktop", "Files", &desktop);
        let xbel = format!(
            r#"<xbel>
  <bookmark href="file://{opened}" modified="2026-01-01T00:00:00Z"><info><metadata><bookmark:applications>
    <bookmark:application name="org.gnome.Nautilus" exec="&apos;feh --start-at %u&apos;" modified="2026-01-01T00:00:00Z"/>
  </bookmark:applications></metadata></info></bookmark>
  <bookmark href="file://{opened}" modified="2026-01-02T00:00:00Z"><info><metadata><bookmark:applications>
    <bookmark:application name="org.gnome.Nautilus" exec="&apos;gio open %u&apos;" modified="2026-01-02T00:00:00Z"/>
  </bookmark:applications></metadata></info></bookmark>
</xbel>"#,
            opened = opened.display()
        );
        std::fs::write(data.join("recently-used.xbel"), xbel).unwrap();
        let documents = for_app(&app, &environment(home));
        // Only the `gio open` record names Files; the `feh` record belongs to
        // feh even though the portal wrote Files' name beside it.
        assert_eq!(documents.len(), 1, "{documents:?}");
        assert_eq!(documents[0].path, opened);
    }

    #[test]
    fn separate_products_with_a_shared_word_do_not_share_recents() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let data = home.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let party_file = home.join("clash-party.png");
        let nyanpasu_file = home.join("config.yml");
        std::fs::write(&party_file, b"png").unwrap();
        std::fs::write(&nyanpasu_file, b"yaml").unwrap();
        let xbel = format!(
            r#"<xbel>
  <bookmark href="file://{party}" modified="2026-01-01T00:00:00Z"><info><metadata><bookmark:applications>
    <bookmark:application name="party" exec="&apos;gio open %u&apos;" modified="2026-01-01T00:00:00Z"/>
  </bookmark:applications></metadata></info></bookmark>
  <bookmark href="file://{nyanpasu}" modified="2026-01-02T00:00:00Z"><info><metadata><bookmark:applications>
    <bookmark:application name="clash-nyanpasu" exec="&apos;clash-nyanpasu %u&apos;" modified="2026-01-02T00:00:00Z"/>
  </bookmark:applications></metadata></info></bookmark>
</xbel>"#,
            party = party_file.display(),
            nyanpasu = nyanpasu_file.display()
        );
        std::fs::write(data.join("recently-used.xbel"), xbel).unwrap();
        let party = home.join("clash-party.desktop");
        std::fs::write(
            &party,
            "[Desktop Entry]\nName=Clash Party\nExec=clash-party %U\nStartupWMClass=mihomo-party\n",
        )
        .unwrap();
        let party_documents = for_app(
            &app("clash-party.desktop", "Clash Party", &party),
            &environment(home),
        );
        assert_eq!(party_documents.len(), 1, "{party_documents:?}");
        assert_eq!(party_documents[0].path, party_file);
        let nyanpasu = home.join("clash-nyanpasu.desktop");
        std::fs::write(
            &nyanpasu,
            "[Desktop Entry]\nName=nyanpasu\nExec=/usr/bin/clash-nyanpasu %u\n",
        )
        .unwrap();
        let nyanpasu_documents = for_app(
            &app("clash-nyanpasu-handler.desktop", "nyanpasu", &nyanpasu),
            &environment(home),
        );
        assert_eq!(nyanpasu_documents.len(), 1, "{nyanpasu_documents:?}");
        assert_eq!(nyanpasu_documents[0].path, nyanpasu_file);
    }

    #[test]
    fn xbel_bookmarks_match_the_app_and_are_newest_first() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let data = home.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let project = home.join("codelib").join("clarion");
        std::fs::create_dir_all(&project).unwrap();
        let notes = home.join("Documents").join("notes.md");
        std::fs::create_dir_all(notes.parent().unwrap()).unwrap();
        std::fs::write(&notes, b"notes").unwrap();
        let old = home.join("codelib").join("old");
        std::fs::create_dir_all(&old).unwrap();
        let missing = home.join("codelib").join("missing");
        let desktop = home.join("zed.desktop");
        std::fs::write(
            &desktop,
            "[Desktop Entry]\nName=Zed\nExec=zeditor %U\n[Desktop Action new]\nName=New\nExec=zeditor\n",
        )
        .unwrap();
        let encoded = notes
            .to_string_lossy()
            .replace(' ', "%20")
            .replace("notes.md", "notes%2Emd");
        let xbel = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0">
  <bookmark href="file://{old}" added="2024-01-01T00:00:00Z" modified="2024-01-01T00:00:00Z" visited="2024-01-01T00:00:00Z">
    <info><metadata owner="http://freedesktop.org">
      <bookmark:applications>
        <bookmark:application name="zed-editor" exec="&apos;gio open %u&apos;" modified="2024-01-01T00:00:00Z" count="1"/>
      </bookmark:applications>
    </metadata></info>
  </bookmark>
  <bookmark href="file://{project}" added="2025-01-01T00:00:00Z" modified="2026-11-17T10:00:00.500Z" visited="2026-11-17T10:00:00Z">
    <info><metadata owner="http://freedesktop.org">
      <bookmark:applications>
        <bookmark:application name="org.gnome.Nautilus" exec="&apos;nautilus %u&apos;" modified="2026-11-18T00:00:00Z" count="1"/>
        <bookmark:application name="zed-editor" exec="&apos;gio open %u&apos;" modified="2026-11-17T10:00:00Z" count="2"/>
      </bookmark:applications>
    </metadata></info>
  </bookmark>
  <bookmark href="file://{encoded}" added="2025-06-01T00:00:00Z" modified="2025-06-01T00:00:00Z" visited="2025-06-01T00:00:00Z">
    <info><metadata owner="http://freedesktop.org">
      <bookmark:applications>
        <bookmark:application name="zed-editor" exec="&apos;gio open %u&apos;" modified="2025-06-01T00:00:00Z" count="1"/>
      </bookmark:applications>
    </metadata></info>
  </bookmark>
  <bookmark href="file://{missing}" added="2026-09-01T00:00:00Z" modified="2026-09-01T00:00:00Z" visited="2026-09-01T00:00:00Z">
    <info><metadata owner="http://freedesktop.org">
      <bookmark:applications>
        <bookmark:application name="zed-editor" exec="&apos;gio open %u&apos;" modified="2026-09-01T00:00:00Z" count="1"/>
      </bookmark:applications>
    </metadata></info>
  </bookmark>
  <bookmark href="file://{project}" added="2023-01-01T00:00:00Z" modified="2023-01-01T00:00:00Z" visited="2023-01-01T00:00:00Z">
    <info><metadata owner="http://freedesktop.org">
      <bookmark:applications>
        <bookmark:application name="zed-editor" exec="&apos;gio open %u&apos;" modified="2023-01-01T00:00:00Z" count="1"/>
      </bookmark:applications>
    </metadata></info>
  </bookmark>
</xbel>"#,
            old = old.display(),
            project = project.display(),
            encoded = encoded,
            missing = missing.display(),
        );
        std::fs::write(data.join("recently-used.xbel"), xbel).unwrap();
        let app = app("dev.zed.Zed.desktop", "Zed", &desktop);
        let documents = for_app(&app, &environment(home));
        assert_eq!(documents.len(), 3, "{documents:?}");
        assert_eq!(documents[0].path, project);
        assert_eq!(documents[0].name, "clarion");
        assert_eq!(documents[0].parent, "~/codelib");
        assert_eq!(documents[1].path, notes);
        assert_eq!(documents[1].name, "notes.md");
        assert_eq!(documents[1].parent, "~/Documents");
        assert_eq!(documents[2].path, old);
        assert_eq!(documents[2].parent, "~/codelib");
    }

    #[test]
    fn another_application_does_not_inherit_the_bookmarks() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let data = home.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let file = home.join("a.txt");
        std::fs::write(&file, b"a").unwrap();
        let desktop = home.join("editor.desktop");
        std::fs::write(&desktop, "[Desktop Entry]\nName=Editor\nExec=editor %U\n").unwrap();
        std::fs::write(
            data.join("recently-used.xbel"),
            format!(
                r#"<xbel><bookmark href="file://{}" modified="2026-01-01T00:00:00Z">
<info><metadata><bookmark:applications>
<bookmark:application name="other-editor" exec="&apos;other %u&apos;" modified="2026-01-01T00:00:00Z"/>
</bookmark:applications></metadata></info></bookmark></xbel>"#,
                file.display()
            ),
        )
        .unwrap();
        let documents = for_app(
            &app("io.github.editor.desktop", "Editor", &desktop),
            &environment(home),
        );
        assert!(documents.is_empty(), "{documents:?}");
    }

    #[test]
    fn jetbrains_self_contained_install_supplies_project_paths() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let install = home.join("software").join("idea");
        std::fs::create_dir_all(install.join("bin")).unwrap();
        std::fs::write(
            install.join("bin").join("idea.properties"),
            "# idea.config.path=${user.home}/.IntelliJIdea/config\n\
             idea.config.path=${user.home}/software/idea/runtime/config\n",
        )
        .unwrap();
        let config = install.join("runtime").join("config").join("options");
        std::fs::create_dir_all(&config).unwrap();
        let alpha = home.join("codelib").join("alpha");
        let beta = home.join("codelib").join("beta");
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::create_dir_all(&beta).unwrap();
        std::fs::write(
            config.join("recentProjects.xml"),
            r#"<application><component name="RecentProjectsManager">
  <option name="additionalInfo"><map>
    <entry key="$USER_HOME$/codelib/beta"><value><RecentProjectMetaInfo>
      <option name="activationTimestamp" value="1000" />
    </RecentProjectMetaInfo></value></entry>
    <entry key="$USER_HOME$/codelib/alpha"><value><RecentProjectMetaInfo>
      <option name="activationTimestamp" value="2000" />
    </RecentProjectMetaInfo></value></entry>
    <entry key="$USER_HOME$/codelib/ghost"><value><RecentProjectMetaInfo>
      <option name="activationTimestamp" value="3000" />
    </RecentProjectMetaInfo></value></entry>
  </map></option>
</component></application>"#,
        )
        .unwrap();
        let desktop = home.join("jetbrains-idea.desktop");
        std::fs::write(
            &desktop,
            format!(
                "[Desktop Entry]\nName=IntelliJ IDEA Ultimate Edition\nExec=\"{}\" %f\n",
                install.join("bin").join("idea").display()
            ),
        )
        .unwrap();
        let documents = for_app(
            &app(
                "jetbrains-idea.desktop",
                "IntelliJ IDEA Ultimate Edition",
                &desktop,
            ),
            &environment(home),
        );
        assert_eq!(documents.len(), 2, "{documents:?}");
        assert_eq!(documents[0].path, alpha);
        assert_eq!(documents[0].name, "alpha");
        assert_eq!(documents[0].parent, "~/codelib");
        assert_eq!(documents[1].path, beta);
    }

    #[test]
    fn jetbrains_products_read_only_their_own_config_directory() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let config_root = home.join("config").join("JetBrains");
        let idea_options = config_root.join("IntelliJIdea2025.2").join("options");
        let pycharm_options = config_root.join("PyCharm2025.2").join("options");
        std::fs::create_dir_all(&idea_options).unwrap();
        std::fs::create_dir_all(&pycharm_options).unwrap();
        let java = home.join("codelib").join("java");
        let python = home.join("codelib").join("python");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::create_dir_all(&python).unwrap();
        write_recent_projects(&idea_options.join("recentProjects.xml"), &java, 10_000);
        write_recent_projects(
            &pycharm_options.join("recentProjects.xml"),
            &python,
            20_000,
        );
        let desktop = home.join("pycharm.desktop");
        std::fs::write(
            &desktop,
            "[Desktop Entry]\nName=PyCharm\nExec=\"/usr/bin/pycharm\" %f\n",
        )
        .unwrap();
        let documents = for_app(
            &app("jetbrains-pycharm.desktop", "PyCharm", &desktop),
            &environment(home),
        );
        assert_eq!(documents.len(), 1, "{documents:?}");
        assert_eq!(documents[0].path, python);
    }

    fn write_recent_projects(file: &Path, project: &Path, at: u64) {
        std::fs::write(
            file,
            format!(
                r#"<application><component name="RecentProjectsManager">
  <option name="additionalInfo"><map>
    <entry key="{}"><value><RecentProjectMetaInfo>
      <option name="activationTimestamp" value="{at}" />
    </RecentProjectMetaInfo></value></entry>
  </map></option>
</component></application>"#,
                project.display()
            ),
        )
        .unwrap();
    }

    #[test]
    fn code_workspace_storage_supplies_project_folders() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let config = home.join("config");
        let project = home.join("codelib").join("clarion");
        std::fs::create_dir_all(&project).unwrap();
        let desktop = home.join("code.desktop");
        std::fs::write(&desktop, "[Desktop Entry]\nName=Visual Studio Code\nExec=code %F\n").unwrap();
        let storage = config.join("Code").join("User").join("workspaceStorage");
        for (hash, folder) in [("aaa", &project), ("bbb", &project)] {
            let directory = storage.join(hash);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join("workspace.json"),
                format!(r#"{{"folder":"file://{}"}}"#, folder.display()),
            )
            .unwrap();
        }
        let documents = for_app(
            &app("com.microsoft.VSCode.desktop", "Visual Studio Code", &desktop),
            &environment(home),
        );
        assert_eq!(documents.len(), 1, "{documents:?}");
        assert_eq!(documents[0].path, project);
        assert_eq!(documents[0].parent, "~/codelib");
    }
}

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub const APP_ID: &str = "io.github.shripad-jyothinath.procharbor";

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ProcessCategory {
    App,
    Background,
    System,
}

impl ProcessCategory {
    pub fn label(self) -> &'static str {
        match self {
            ProcessCategory::App => "Apps",
            ProcessCategory::Background => "Background",
            ProcessCategory::System => "System",
        }
    }
}

pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
}

pub fn parse_desktop_entry(content: &str) -> Option<HashMap<String, String>> {
    let mut in_desktop_entry = false;
    let mut found_section = false;
    let mut map = HashMap::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let sec = &line[1..line.len() - 1];
            if sec == "Desktop Entry" {
                in_desktop_entry = true;
                found_section = true;
            } else {
                in_desktop_entry = false;
            }
            continue;
        }
        if in_desktop_entry {
            if let Some((k, v)) = line.split_once('=') {
                map.entry(k.trim().to_string())
                    .or_insert_with(|| v.trim().to_string());
            }
        }
    }

    if found_section { Some(map) } else { None }
}

pub fn split_shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut has_token = false;

    for ch in input.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            has_token = true;
            continue;
        }
        if ch == '\\' && !in_single {
            escaped = true;
            has_token = true;
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
            has_token = true;
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
            has_token = true;
            continue;
        }
        if ch.is_whitespace() && !in_single && !in_double {
            if has_token {
                words.push(std::mem::take(&mut cur));
                has_token = false;
            }
        } else {
            cur.push(ch);
            has_token = true;
        }
    }
    if has_token {
        words.push(cur);
    }
    words
}

pub struct DesktopIndex {
    icon_exact: HashMap<String, String>,
    icon_prefixes: Vec<(String, String)>,
    icon_cache: HashMap<String, String>,
    desktop_cache: HashMap<String, bool>,
}

impl DesktopIndex {
    pub fn build<F>(mut has_icon: F) -> Self
    where
        F: FnMut(&str) -> bool,
    {
        let home = home_dir();
        let dirs = [
            format!("{}/.local/share/applications", home),
            "/usr/local/share/applications".to_string(),
            "/usr/share/applications".to_string(),
            "/var/lib/flatpak/exports/share/applications".to_string(),
            "/var/lib/snapd/desktop/applications".to_string(),
        ];

        let mut icon_exact: HashMap<String, String> = HashMap::new();
        let mut prefixes: HashMap<String, String> = HashMap::new();

        for base in &dirs {
            let Ok(entries) = fs::read_dir(base) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".desktop"))
                .collect();
            names.sort();

            for fn_name in names {
                let path = Path::new(base).join(&fn_name);
                let Ok(bytes) = fs::read(&path) else {
                    continue;
                };
                let text = String::from_utf8_lossy(&bytes);
                let Some(d) = parse_desktop_entry(&text) else {
                    continue;
                };
                if d.get("Type").map(|s| s.as_str()).unwrap_or("Application") != "Application" {
                    continue;
                }
                let icon = d.get("Icon").map(|s| s.trim()).unwrap_or("");
                if icon.is_empty() || (!Path::new(icon).exists() && !has_icon(icon)) {
                    continue;
                }

                let mut tokens = HashSet::new();
                tokens.insert(
                    fn_name
                        .trim_end_matches(".desktop")
                        .to_lowercase()
                        .replace('_', "-"),
                );

                if let Some(wm) = d.get("StartupWMClass") {
                    let wm = wm.trim().to_lowercase();
                    if !wm.is_empty() {
                        tokens.insert(wm.replace('_', "-"));
                    }
                }

                if let Some(exec_line) = d.get("Exec") {
                    let exec_line = exec_line.trim();
                    if !exec_line.is_empty() {
                        let mut parts = split_shell_words(exec_line);
                        while !parts.is_empty() {
                            let base_cmd = Path::new(&parts[0])
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("");
                            if base_cmd == "env" || parts[0].contains('=') {
                                parts.remove(0);
                            } else {
                                break;
                            }
                        }
                        if let Some(first) = parts.first() {
                            let exe = Path::new(first)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            if !exe.is_empty() {
                                tokens.insert(exe.replace('_', "-"));
                            }
                            if exe == "flatpak" {
                                for part in parts.iter().skip(1) {
                                    if !part.starts_with('-') && part != "run" {
                                        tokens.insert(part.to_lowercase().replace('_', "-"));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                for tok in tokens {
                    if tok.len() < 3 {
                        continue;
                    }
                    icon_exact
                        .entry(tok.clone())
                        .or_insert_with(|| icon.to_string());
                    prefixes.entry(tok).or_insert_with(|| icon.to_string());
                }
            }
        }

        let mut icon_prefixes: Vec<(String, String)> = prefixes.into_iter().collect();
        icon_prefixes.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

        Self {
            icon_exact,
            icon_prefixes,
            icon_cache: HashMap::new(),
            desktop_cache: HashMap::new(),
        }
    }

    pub fn icon_for_app(&mut self, app: &str, exe: &str, cmdline: &str) -> String {
        let app_l = app.trim().to_lowercase();
        let exe_base = Path::new(exe.trim_end_matches('/'))
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let cmd_l = cmdline.to_lowercase();
        if (cmd_l.contains("procharbor") || app_l == "procharbor")
            && (app_l == "procharbor" || app_l.starts_with("python"))
        {
            return APP_ID.to_string();
        }

        let key = if !app_l.is_empty() {
            app_l.clone()
        } else if !exe_base.is_empty() {
            exe_base.clone()
        } else {
            "?".to_string()
        };

        if let Some(cached) = self.icon_cache.get(&key) {
            return cached.clone();
        }

        let candidates: Vec<String> = [&app_l, &exe_base]
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.replace('_', "-"))
            .collect();

        for cand in &candidates {
            if let Some(icon) = self.icon_exact.get(cand) {
                self.icon_cache.insert(key, icon.clone());
                return icon.clone();
            }
        }

        let mut best: Option<(usize, &str)> = None;
        for (token, icon) in &self.icon_prefixes {
            for cand in &candidates {
                if cand.len() < 4 {
                    continue;
                }
                if cand.starts_with(token.as_str()) || token.starts_with(cand.as_str()) {
                    let score = token.len().min(cand.len());
                    if best.is_none_or(|(b, _)| score > b) {
                        best = Some((score, icon.as_str()));
                    }
                }
            }
        }
        if let Some((_, icon)) = best {
            let icon_s = icon.to_string();
            self.icon_cache.insert(key, icon_s.clone());
            return icon_s;
        }

        let icon = if exe_base.is_empty()
            || [
                "kworker",
                "ksoftirqd",
                "migration",
                "rcu",
                "irq/",
                "idle_inject",
                "cpuhp",
                "kthreadd",
                "kprobe",
                "kcompactd",
                "kswapd",
                "kdevtmpfs",
                "kauditd",
                "khungtaskd",
                "kintegrityd",
                "kblockd",
                "kthrotld",
                "scsi_eh",
                "nvme-",
                "i915",
                "card0-",
                "watchdog",
            ]
            .iter()
            .any(|p| app_l.starts_with(p))
            || app_l.starts_with("systemd")
            || matches!(
                app_l.as_str(),
                "dbus-daemon"
                    | "dbus-broker"
                    | "polkitd"
                    | "rtkit-daemon"
                    | "accounts-daemon"
                    | "avahi-daemon"
                    | "colord"
                    | "cupsd"
                    | "cups-browsed"
                    | "udisksd"
                    | "upowerd"
                    | "wpa_supplicant"
                    | "networkmanager"
                    | "snapd"
                    | "packagekitd"
            ) {
            "applications-system-symbolic"
        } else if matches!(
            app_l.as_str(),
            "bash"
                | "zsh"
                | "fish"
                | "sh"
                | "dash"
                | "tmux"
                | "screen"
                | "python"
                | "python3"
                | "python3.10"
                | "python3.11"
                | "python3.12"
                | "python3.13"
                | "node"
                | "npm"
                | "npx"
                | "git"
                | "cargo"
                | "rustc"
                | "make"
                | "gcc"
                | "clang"
                | "java"
                | "perl"
                | "ruby"
                | "php"
        ) {
            "utilities-terminal-symbolic"
        } else if matches!(
            app_l.as_str(),
            "gnome-shell" | "gnome-software" | "gnome-control-center"
        ) {
            "utilities-system-monitor-symbolic"
        } else {
            "application-x-executable-symbolic"
        };

        let res = icon.to_string();
        self.icon_cache.insert(key, res.clone());
        res
    }

    pub fn has_desktop_match(&mut self, app: &str, exe: &str, cmdline: &str) -> bool {
        let app_l = app.trim().to_lowercase();
        let cmd_l = cmdline.to_lowercase();
        if (app_l == "procharbor")
            || (app_l.starts_with("python") && cmd_l.contains("procharbor.py"))
        {
            return true;
        }
        let exe_base = Path::new(exe.trim_end_matches('/'))
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let key = if !app_l.is_empty() {
            app_l.clone()
        } else if !exe_base.is_empty() {
            exe_base.clone()
        } else {
            "?".to_string()
        };

        if let Some(&cached) = self.desktop_cache.get(&key) {
            return cached;
        }

        let candidates: Vec<String> = [&app_l, &exe_base]
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.replace('_', "-"))
            .collect();

        let mut result = candidates.iter().any(|c| self.icon_exact.contains_key(c));
        if !result {
            'outer: for (token, _) in &self.icon_prefixes {
                for cand in &candidates {
                    if cand.len() >= 4
                        && (cand.starts_with(token.as_str()) || token.starts_with(cand.as_str()))
                    {
                        result = true;
                        break 'outer;
                    }
                }
            }
        }
        self.desktop_cache.insert(key, result);
        result
    }

    pub fn process_category(
        &mut self,
        uid: u32,
        app: &str,
        exe: &str,
        cmdline: &str,
        user_uids: &HashSet<u32>,
    ) -> ProcessCategory {
        if !user_uids.contains(&uid) {
            return ProcessCategory::System;
        }
        let name = app.to_lowercase().replace('_', "-");
        if name == "procharbor"
            || (name.starts_with("python") && cmdline.to_lowercase().contains("procharbor.py"))
        {
            return ProcessCategory::App;
        }
        if matches!(
            name.as_str(),
            "sh" | "bash"
                | "dash"
                | "zsh"
                | "fish"
                | "ksh"
                | "csh"
                | "tcsh"
                | "tmux"
                | "screen"
                | "python"
                | "python3"
                | "python3.10"
                | "python3.11"
                | "python3.12"
                | "python3.13"
                | "node"
                | "npm"
                | "npx"
                | "perl"
                | "ruby"
                | "php"
                | "java"
        ) {
            return ProcessCategory::Background;
        }
        if [
            "-daemon",
            "-service",
            "-server",
            "-agent",
            "-portal",
            "-registry",
            "-factory",
            "-notify",
            "-helper",
            "-launcher",
            "-bus",
            "-extension",
            "-engine",
            "-memconf",
            "-host",
        ]
        .iter()
        .any(|suf| name.ends_with(suf))
        {
            return ProcessCategory::Background;
        }
        let exe_l = exe.to_lowercase();
        if exe_l.starts_with("/usr/libexec/")
            || exe_l.starts_with("/usr/lib/systemd/")
            || exe_l.starts_with("/lib/systemd/")
        {
            return ProcessCategory::Background;
        }
        if self.has_desktop_match(app, exe, cmdline) {
            ProcessCategory::App
        } else {
            ProcessCategory::Background
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_desktop_entry() {
        let content = r#"
[Desktop Entry]
Type=Application
Name=My App
Exec="/usr/bin/my app" --flag %U
Icon=my-app
Hidden=false

[Desktop Action New]
Name=New Window
"#;
        let map = parse_desktop_entry(content).unwrap();
        assert_eq!(map.get("Type").unwrap(), "Application");
        assert_eq!(map.get("Name").unwrap(), "My App");
        assert_eq!(map.get("Icon").unwrap(), "my-app");
    }

    #[test]
    fn test_split_shell_words() {
        let words = split_shell_words(r#"env FOO=1 "/usr/bin/brave browser" --flag='a b'"#);
        assert_eq!(
            words,
            vec!["env", "FOO=1", "/usr/bin/brave browser", "--flag=a b"]
        );
    }

    #[test]
    fn test_process_category_rules() {
        let mut idx = DesktopIndex {
            icon_exact: HashMap::from([("firefox".to_string(), "firefox".to_string())]),
            icon_prefixes: vec![],
            icon_cache: HashMap::new(),
            desktop_cache: HashMap::new(),
        };
        let user_uids = HashSet::from([1000]);

        assert_eq!(
            idx.process_category(0, "systemd", "/lib/systemd/systemd", "", &user_uids),
            ProcessCategory::System
        );
        assert_eq!(
            idx.process_category(1000, "bash", "/bin/bash", "bash", &user_uids),
            ProcessCategory::Background
        );
        assert_eq!(
            idx.process_category(
                1000,
                "gnome-keyring-daemon",
                "/usr/bin/gnome-keyring-daemon",
                "",
                &user_uids
            ),
            ProcessCategory::Background
        );
        assert_eq!(
            idx.process_category(
                1000,
                "firefox",
                "/usr/lib/firefox/firefox",
                "firefox",
                &user_uids
            ),
            ProcessCategory::App
        );
        assert_eq!(
            idx.process_category(
                1000,
                "procharbor",
                "/usr/bin/procharbor",
                "procharbor",
                &user_uids
            ),
            ProcessCategory::App
        );
    }
}

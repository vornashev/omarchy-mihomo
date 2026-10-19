//! Running programs, system users and Hyprland windows, for rule
//! suggestions. Everything here is best effort: unreadable entries (other
//! users' processes, no Hyprland) are skipped, never errors.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Program {
    /// What the core matches with PROCESS-NAME: the executable's file name.
    pub name: String,
    pub path: String,
    pub pids: Vec<u32>,
    pub uid: Option<u32>,
    /// Title of a window one of its processes owns.
    pub window: Option<String>,
}

/// Programs running now, one entry per executable.
#[cfg(not(target_os = "macos"))]
pub fn programs(proc_root: &Path) -> Vec<Program> {
    let mut by_path: BTreeMap<String, Program> = BTreeMap::new();
    let Ok(dir) = std::fs::read_dir(proc_root) else { return Vec::new() };
    for entry in dir.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(exe) = std::fs::read_link(entry.path().join("exe")) else { continue };
        let path = exe.to_string_lossy().trim_end_matches(" (deleted)").to_owned();
        let Some(name) = Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        let uid = std::fs::read_to_string(entry.path().join("status"))
            .ok()
            .and_then(|s| s.lines().find_map(|l| l.strip_prefix("Uid:").and_then(|r| r.split_whitespace().next()).and_then(|u| u.parse().ok())));
        let p = by_path.entry(path.clone()).or_insert_with(|| Program { name, path, pids: Vec::new(), uid, window: None });
        p.pids.push(pid);
    }
    by_path.into_values().collect()
}

/// `(uid, name)` of login users and root, from a passwd file.
pub fn users(passwd: &str) -> Vec<(u32, String)> {
    passwd
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let uid: u32 = f.get(2)?.parse().ok()?;
            let shell = f.get(6).copied().unwrap_or_default();
            let login = !shell.ends_with("nologin") && !shell.ends_with("false");
            (uid == 0 || ((1000..60000).contains(&uid) && login)).then(|| (uid, f[0].to_owned()))
        })
        .collect()
}

/// Window titles by pid, from `hyprctl clients -j`.
#[cfg(not(target_os = "macos"))]
pub async fn windows() -> BTreeMap<u32, String> {
    let run = tokio::process::Command::new("hyprctl").args(["clients", "-j"]).kill_on_drop(true).output();
    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_millis(600), run).await else { return BTreeMap::new() };
    parse_clients(&out.stdout)
}

fn parse_clients(json: &[u8]) -> BTreeMap<u32, String> {
    let Ok(serde_json::Value::Array(clients)) = serde_json::from_slice(json) else { return BTreeMap::new() };
    clients
        .iter()
        .filter_map(|c| {
            let pid = c.get("pid")?.as_u64()? as u32;
            let title = c.get("title").and_then(|t| t.as_str()).filter(|t| !t.is_empty()).or_else(|| c.get("class")?.as_str())?;
            Some((pid, title.to_owned()))
        })
        .collect()
}

/// Attach window titles; a window owned by a child process (browsers,
/// Electron) is found through the parent chain.
pub fn attach_windows(programs: &mut [Program], windows: &BTreeMap<u32, String>, proc_root: &Path) {
    if windows.is_empty() {
        return;
    }
    let parent = |pid: u32| -> Option<u32> {
        let stat = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
        stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
    };
    for p in programs.iter_mut() {
        p.window = p.pids.iter().find_map(|&pid| {
            let mut cur = pid;
            for _ in 0..4 {
                if let Some(t) = windows.get(&cur) {
                    return Some(t.clone());
                }
                cur = parent(cur).filter(|&pp| pp > 1)?;
            }
            None
        });
    }
}

/// A process under a window's process, for "rule for the active window".
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Node {
    pub pid: u32,
    pub name: String,
    pub path: String,
    /// 0 for the window's own process.
    pub depth: u32,
    pub connections: usize,
}

/// `pid` and its descendants (a terminal and what runs in it), closest
/// first. Processes without a readable executable are skipped.
#[cfg(not(target_os = "macos"))]
pub fn tree(proc_root: &Path, pid: u32) -> Vec<Node> {
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    if let Ok(dir) = std::fs::read_dir(proc_root) {
        for entry in dir.flatten() {
            let Some(child) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else { continue };
            let Some(parent) = stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)?.parse::<u32>().ok()) else { continue };
            children.entry(parent).or_default().push(child);
        }
    }
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::from([(pid, 0u32)]);
    while let Some((p, depth)) = queue.pop_front() {
        if out.len() >= 64 {
            break;
        }
        if let Ok(exe) = std::fs::read_link(proc_root.join(p.to_string()).join("exe")) {
            let path = exe.to_string_lossy().trim_end_matches(" (deleted)").to_owned();
            let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            out.push(Node { pid: p, name, path, depth, connections: 0 });
        }
        if depth < 4 {
            for &c in children.get(&p).into_iter().flatten() {
                queue.push_back((c, depth + 1));
            }
        }
    }
    out
}

#[cfg(target_os = "macos")]
pub fn tree(_: &Path, _: u32) -> Vec<Node> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_starts_at_the_process() {
        let me = std::process::id();
        let t = tree(Path::new("/proc"), me);
        assert_eq!(t.first().map(|n| (n.pid, n.depth)), Some((me, 0)));
    }

    #[test]
    fn lists_own_processes() {
        let me = std::process::id();
        let list = programs(Path::new("/proc"));
        assert!(list.iter().any(|p| p.pids.contains(&me) && !p.name.is_empty() && p.path.starts_with('/')));
    }

    #[test]
    fn login_users_only() {
        let passwd = "root:x:0:0::/root:/bin/bash\nbin:x:1:1::/:/usr/bin/nologin\ntema:x:1000:1000::/home/tema:/bin/zsh\nnobody:x:65534:65534::/:/usr/bin/nologin\n";
        assert_eq!(users(passwd), [(0, "root".to_owned()), (1000, "tema".to_owned())]);
    }

    #[test]
    fn hyprland_clients() {
        let json = br#"[{"pid":2311,"class":"org.telegram.desktop","title":"Telegram"},{"pid":12,"class":"kitty","title":""},{"class":"x"}]"#;
        let w = parse_clients(json);
        assert_eq!(w.get(&2311).map(String::as_str), Some("Telegram"));
        assert_eq!(w.get(&12).map(String::as_str), Some("kitty"));
        assert_eq!(w.len(), 2);
    }
}

#[cfg(target_os = "macos")]
pub fn programs(_: &Path) -> Vec<Program> {
    omm_platform::macos::processes().into_iter().map(|(path,pids)| {
        let name = Path::new(&path).file_name().unwrap_or_default().to_string_lossy().into_owned();
        Program { name, path, pids, uid:None, window:None }
    }).collect()
}
#[cfg(target_os = "macos")]
pub async fn windows() -> BTreeMap<u32,String> { BTreeMap::new() }

//! 文件收集：把用户给的路径（文件或目录）展开成待处理的图片文件列表。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// 根据扩展名过滤集合与递归开关，从若干输入路径收集图片文件。
///
/// - 直接给出的文件即使扩展名不在集合内也会被收录（用户明确指定即处理）。
/// - 目录会被展开；`recursive` 决定是否深入子目录。
/// - 结果去重并按路径排序，保证序列模式下顺序稳定、可预测。
pub fn collect_files(inputs: &[PathBuf], exts: &HashSet<String>, recursive: bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for input in inputs {
        if input.is_file() {
            push_unique(&mut out, &mut seen, input.clone());
        } else if input.is_dir() {
            let max_depth = if recursive { usize::MAX } else { 1 };
            for entry in WalkDir::new(input)
                .max_depth(max_depth)
                .sort_by_file_name()
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.is_file() && ext_matches(p, exts) {
                    push_unique(&mut out, &mut seen, p.to_path_buf());
                }
            }
        } else if has_wildcard(input) {
            // 内置通配符展开（Windows 的 cmd/PowerShell 不会为外部程序展开 *.jpg）
            let mut matches = expand_glob(input);
            matches.sort();
            for p in matches {
                push_unique(&mut out, &mut seen, p);
            }
        }
        // 不存在的路径静默跳过，由调用方统计报告
    }

    out.sort();
    out
}

fn has_wildcard(path: &Path) -> bool {
    path.to_str()
        .map(|s| s.contains(['*', '?']))
        .unwrap_or(false)
}

/// 展开形如 `*.jpg`、`photos/IMG_*.jpg`、`p?.png` 的通配符（仅文件名部分，单层）。
/// 匹配到的文件按原样收录（不再按扩展名过滤，模式本身即用户的筛选）。
fn expand_glob(pattern: &Path) -> Vec<PathBuf> {
    let dir = pattern
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let Some(name_pat) = pattern.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(fname) = entry.file_name().to_str()
                && glob_match(name_pat, fname)
            {
                let p = entry.path();
                if p.is_file() {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// 经典通配符匹配：`*` 匹配任意（含空），`?` 匹配单个字符；大小写不敏感（贴合 Windows）。
fn glob_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.to_lowercase().chars().collect();
    let txt: Vec<char> = name.to_lowercase().chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while t < txt.len() {
        if p < pat.len() && (pat[p] == '?' || pat[p] == txt[t]) {
            p += 1;
            t += 1;
        } else if p < pat.len() && pat[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(sp) = star {
            p = sp + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == '*' {
        p += 1;
    }
    p == pat.len()
}

fn push_unique(out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, p: PathBuf) {
    let key = p.canonicalize().unwrap_or_else(|_| p.clone());
    if seen.insert(key) {
        out.push(p);
    }
}

fn ext_matches(path: &Path, exts: &HashSet<String>) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| exts.contains(&e.to_ascii_lowercase()))
        .unwrap_or(false)
}

/// 把逗号分隔的扩展名字符串解析成小写集合（去掉前导点与空白）。
pub fn parse_ext_set(s: &str) -> HashSet<String> {
    s.split(',')
        .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_star() {
        assert!(glob_match("*.jpg", "photo.jpg"));
        assert!(glob_match("*.jpg", "a.JPG")); // 大小写不敏感
        assert!(!glob_match("*.jpg", "photo.png"));
        assert!(glob_match("IMG_*.jpg", "IMG_1234.jpg"));
        assert!(!glob_match("IMG_*.jpg", "DSC_1234.jpg"));
    }

    #[test]
    fn glob_question() {
        assert!(glob_match("p?.png", "p1.png"));
        assert!(!glob_match("p?.png", "p12.png"));
    }

    #[test]
    fn glob_edge() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("**", "anything")); // 多个 * 也可
        assert!(glob_match("a*b*c", "axxbyyc"));
        assert!(!glob_match("a*b*c", "axxbyy"));
        assert!(glob_match("abc", "abc"));
        assert!(!glob_match("abc", "abcd"));
    }
}

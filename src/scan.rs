//! 文件收集：把用户给的路径（文件或目录）展开成待处理的图片文件列表。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// 根据扩展名过滤集合与递归开关，从若干输入路径收集图片文件。
///
/// - 直接给出的文件即使扩展名不在集合内也会被收录（用户明确指定即处理）。
/// - 目录会被展开；`recursive` 决定是否深入子目录。
/// - 结果去重并按路径排序，保证序列模式下顺序稳定、可预测。
pub struct Collected {
    /// 实际收集到的图片文件，已去重并按自然顺序排序。
    pub files: Vec<PathBuf>,
    /// 用户明确给出、但磁盘上不存在的路径（不含匹配为空的通配符）。
    pub missing: Vec<PathBuf>,
}

pub fn collect_files(inputs: &[PathBuf], exts: &HashSet<String>, recursive: bool) -> Collected {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut missing: Vec<PathBuf> = Vec::new();

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
            matches.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
            for p in matches {
                push_unique(&mut out, &mut seen, p);
            }
        } else {
            // 打错的路径以前会被静默丢掉，退出码还是 0：你以为处理了 200 张，
            // 其实一张都没碰。通配符匹配为空不算——那是模式，不是路径。
            missing.push(input.clone());
        }
    }

    out.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    Collected {
        files: out,
        missing,
    }
}

/// 自然顺序比较：数字段按数值比，其余按字节比。
///
/// 纯字节序会把 `IMG_10.jpg` 排在 `IMG_2.jpg` 前面。对 `show` 只是不好看，
/// 对 `time --sequential` 就是错的——它按这个顺序给照片递增时间戳。
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    loop {
        match (x.first(), y.first()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(cx), Some(cy)) => {
                if cx.is_ascii_digit() && cy.is_ascii_digit() {
                    let nx = x
                        .iter()
                        .position(|c| !c.is_ascii_digit())
                        .unwrap_or(x.len());
                    let ny = y
                        .iter()
                        .position(|c| !c.is_ascii_digit())
                        .unwrap_or(y.len());
                    // 先比去掉前导零后的长度，再逐位比，避免解析成整数时溢出。
                    let dx = trim_zeros(&x[..nx]);
                    let dy = trim_zeros(&y[..ny]);
                    let ord = dx.len().cmp(&dy.len()).then_with(|| dx.cmp(dy));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    x = &x[nx..];
                    y = &y[ny..];
                } else {
                    let ord = cx.cmp(cy);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    x = &x[1..];
                    y = &y[1..];
                }
            }
        }
    }
}

fn trim_zeros(d: &[u8]) -> &[u8] {
    let start = d.iter().position(|c| *c != b'0').unwrap_or(d.len());
    &d[start..]
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
    fn natural_order_numeric_runs() {
        use std::cmp::Ordering;
        assert_eq!(natural_cmp("IMG_2.jpg", "IMG_10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("IMG_10.jpg", "IMG_2.jpg"), Ordering::Greater);
        assert_eq!(natural_cmp("IMG_2.jpg", "IMG_2.jpg"), Ordering::Equal);
        // 前导零不改变数值大小
        assert_eq!(natural_cmp("a007", "a7"), Ordering::Equal);
        assert_eq!(natural_cmp("a008", "a7"), Ordering::Greater);
        // 超出 u64 的数字段也不会溢出或 panic
        assert_eq!(
            natural_cmp("x99999999999999999999999", "x100000000000000000000000"),
            Ordering::Less
        );
        // 非数字部分仍按字节比较
        assert_eq!(natural_cmp("a.jpg", "b.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("IMG_1", "IMG_1a"), Ordering::Less);
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

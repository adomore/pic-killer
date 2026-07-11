//! `--where` 条件筛选：按元数据条件过滤要处理的文件。
//!
//! 支持单个条件（暂不支持 AND/OR 组合）：
//! - 存在性：`has-gps` / `no-gps`、`has-date` / `no-date`、`has-xmp` / `no-xmp`
//! - 标签存在：`has:NAME` / `no:NAME`（NAME 大小写不敏感、子串匹配，覆盖 EXIF/XMP/IPTC）
//! - 标签比较：`NAME=VALUE`、`NAME!=VALUE`、`NAME~VALUE`（含）、`NAME!~VALUE`（不含）

use std::path::Path;

use anyhow::{Result, bail};

use crate::exif;
use crate::iptc;
use crate::xmp;

#[derive(Debug, PartialEq)]
pub enum Op {
    Eq,
    Ne,
    Contains,
    NotContains,
}

#[derive(Debug, PartialEq)]
pub enum Condition {
    HasGps,
    NoGps,
    HasDate,
    NoDate,
    HasXmp,
    NoXmp,
    TagPresence { name: String, present: bool },
    Tag { name: String, op: Op, value: String },
}

/// 解析 `--where` 表达式。
pub fn parse(expr: &str) -> Result<Condition> {
    let e = expr.trim();
    if e.is_empty() {
        bail!("--where 条件为空");
    }
    match e.to_ascii_lowercase().as_str() {
        "has-gps" | "gps" => return Ok(Condition::HasGps),
        "no-gps" => return Ok(Condition::NoGps),
        "has-date" | "date" => return Ok(Condition::HasDate),
        "no-date" => return Ok(Condition::NoDate),
        "has-xmp" | "xmp" => return Ok(Condition::HasXmp),
        "no-xmp" => return Ok(Condition::NoXmp),
        _ => {}
    }
    if let Some(name) = strip_prefix_ci(e, "has:") {
        return Ok(Condition::TagPresence {
            name,
            present: true,
        });
    }
    if let Some(name) = strip_prefix_ci(e, "no:") {
        return Ok(Condition::TagPresence {
            name,
            present: false,
        });
    }

    // 选取出现位置最靠前的运算符（这样 `!=` 会先于其中的 `=` 被识别）
    let ops = [
        ("!=", Op::Ne),
        ("!~", Op::NotContains),
        ("=", Op::Eq),
        ("~", Op::Contains),
    ];
    let mut best: Option<(usize, usize, Op)> = None; // (位置, 运算符长度, Op)
    for (sym, op) in ops {
        if let Some(idx) = e.find(sym) {
            let better = match &best {
                None => true,
                Some((bi, blen, _)) => idx < *bi || (idx == *bi && sym.len() > *blen),
            };
            if better {
                best = Some((idx, sym.len(), op));
            }
        }
    }

    if let Some((idx, len, op)) = best {
        let name = e[..idx].trim().to_string();
        let value = e[idx + len..].trim().to_string();
        if name.is_empty() {
            bail!("--where 条件缺少字段名：`{expr}`");
        }
        return Ok(Condition::Tag { name, op, value });
    }

    bail!(
        "无法解析 --where 条件 `{expr}`，示例：no-gps、has-date、make=Canon、artist~张、has:rating"
    )
}

fn strip_prefix_ci(s: &str, prefix: &str) -> Option<String> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(s[prefix.len()..].trim().to_string())
    } else {
        None
    }
}

impl Condition {
    /// 该文件是否满足条件。无法读取的文件按“不满足存在性”处理。
    pub fn matches(&self, path: &Path) -> bool {
        match self {
            Condition::HasGps => gps_present(path),
            Condition::NoGps => !gps_present(path),
            Condition::HasDate => date_present(path),
            Condition::NoDate => !date_present(path),
            Condition::HasXmp => xmp_present(path),
            Condition::NoXmp => !xmp_present(path),
            Condition::TagPresence { name, present } => tag_present(path, name) == *present,
            Condition::Tag { name, op, value } => eval_tag(path, name, op, value),
        }
    }
}

fn gps_present(path: &Path) -> bool {
    exif::load_metadata(path)
        .ok()
        .and_then(|m| exif::read_gps(&m))
        .is_some()
}

fn date_present(path: &Path) -> bool {
    exif::load_metadata(path)
        .ok()
        .and_then(|m| exif::read_capture_time(&m))
        .is_some()
}

fn xmp_present(path: &Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|b| xmp::extract_packet_bytes(&b))
        .is_some()
}

/// 汇总 EXIF + XMP + IPTC 的 (名称, 值) 列表。
fn all_props(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(m) = exif::load_metadata(path) {
        for t in exif::list_tags(&m) {
            out.push((t.name, t.value));
        }
    }
    if let Ok(bytes) = std::fs::read(path) {
        if let Some(pkt) = xmp::extract_packet_bytes(&bytes)
            && let Ok(s) = std::str::from_utf8(&pkt)
        {
            out.extend(xmp::read_properties(s));
        }
        out.extend(iptc::read_properties(&bytes));
    }
    out
}

fn tag_present(path: &Path, name: &str) -> bool {
    let name_l = name.to_ascii_lowercase();
    all_props(path)
        .iter()
        .any(|(n, _)| n.to_ascii_lowercase().contains(&name_l))
}

fn eval_tag(path: &Path, name: &str, op: &Op, value: &str) -> bool {
    let name_l = name.to_ascii_lowercase();
    let value_l = value.to_ascii_lowercase();
    let vals: Vec<String> = all_props(path)
        .into_iter()
        .filter(|(n, _)| n.to_ascii_lowercase().contains(&name_l))
        .map(|(_, v)| v.to_ascii_lowercase())
        .collect();
    let eq = vals.contains(&value_l);
    let contains = vals.iter().any(|v| v.contains(&value_l));
    match op {
        Op::Eq => eq,
        Op::Ne => !eq,
        Op::Contains => contains,
        Op::NotContains => !contains,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_presence() {
        assert_eq!(parse("no-gps").unwrap(), Condition::NoGps);
        assert_eq!(parse("HAS-GPS").unwrap(), Condition::HasGps);
        assert_eq!(parse(" no-date ").unwrap(), Condition::NoDate);
    }

    #[test]
    fn parse_tag_presence() {
        assert_eq!(
            parse("has:rating").unwrap(),
            Condition::TagPresence {
                name: "rating".into(),
                present: true
            }
        );
        assert_eq!(
            parse("no:GPS").unwrap(),
            Condition::TagPresence {
                name: "GPS".into(),
                present: false
            }
        );
    }

    #[test]
    fn parse_comparisons() {
        assert_eq!(
            parse("make=Canon").unwrap(),
            Condition::Tag {
                name: "make".into(),
                op: Op::Eq,
                value: "Canon".into()
            }
        );
        assert_eq!(
            parse("make!=Canon").unwrap(),
            Condition::Tag {
                name: "make".into(),
                op: Op::Ne,
                value: "Canon".into()
            }
        );
        assert_eq!(
            parse("artist~张").unwrap(),
            Condition::Tag {
                name: "artist".into(),
                op: Op::Contains,
                value: "张".into()
            }
        );
        assert_eq!(
            parse("model!~EOS").unwrap(),
            Condition::Tag {
                name: "model".into(),
                op: Op::NotContains,
                value: "EOS".into()
            }
        );
    }

    #[test]
    fn ne_takes_precedence_over_eq() {
        // "a!=b" 必须识别为 Ne 而不是把 "!" 留在字段名里
        match parse("a!=b").unwrap() {
            Condition::Tag { name, op, value } => {
                assert_eq!(name, "a");
                assert_eq!(op, Op::Ne);
                assert_eq!(value, "b");
            }
            _ => panic!("应解析为 Tag/Ne"),
        }
    }

    #[test]
    fn bad_expr_rejected() {
        assert!(parse("").is_err());
        assert!(parse("=value").is_err());
        assert!(parse("garbage").is_err());
    }
}

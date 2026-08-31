//! `--where` 条件筛选：按元数据条件过滤要处理的文件。
//!
//! 支持单个条件，也支持用 `&&` 或 `||` 组合多个条件（两者不可混用，混用会报错）：
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

/// 多个条件的组合：`A && B`（都满足）或 `A || B`（任一满足）。不支持混用优先级。
pub struct WhereExpr {
    conditions: Vec<Condition>,
    any: bool, // true=OR，false=AND
}

/// 解析组合条件表达式。含 `||` 视为 OR，否则按 `&&` 拆成 AND（单条件也走这里）。
///
/// 不支持 `&&` 与 `||` 混用。混用会被明确拒绝，而不是按其中一种切分后静默误解析——
/// 后者会让 `a && b || c` 变成对一个名叫 `a && b` 的标签做比较，返回错误的文件集合。
pub fn parse_expr(expr: &str) -> Result<WhereExpr> {
    let (sep, other, any) = if expr.contains("||") {
        ("||", "&&", true)
    } else {
        ("&&", "||", false)
    };
    let parts: Vec<&str> = expr
        .split(sep)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    if let Some(bad) = parts.iter().find(|p| p.contains(other)) {
        bail!(
            "--where 不支持 `&&` 与 `||` 混用（在 `{bad}` 处）。请拆成两条命令，\
             或先用 `show` 分别确认两组条件各自选中了哪些文件"
        );
    }
    let conditions = parts.into_iter().map(parse).collect::<Result<Vec<_>>>()?;
    if conditions.is_empty() {
        bail!("--where 条件为空");
    }
    Ok(WhereExpr { conditions, any })
}

/// 一个文件的元数据事实，一次读取得出，供表达式里所有条件复用。
///
/// 之前每个条件各自去读一遍文件：`--where "no-gps && no-date"` 对每张照片
/// 做两次完整加载，`make=Canon && model=X` 更是把整个文件读进来、
/// 把 EXIF/XMP/IPTC 各解析两遍。条件数越多放大得越厉害。
pub struct FileFacts {
    /// (小写名称, 小写值)。预先归一化，省得每个条件各转一次大小写。
    props: Vec<(String, String)>,
    has_gps: bool,
    has_date: bool,
    has_xmp: bool,
}

impl FileFacts {
    /// 读取一个文件并求出全部事实。整份字节只读一次。
    pub fn read(path: &Path) -> Self {
        let mut props: Vec<(String, String)> = Vec::new();
        let (mut has_gps, mut has_date, mut has_xmp) = (false, false, false);

        let bytes = std::fs::read(path).ok();
        if let Some(bytes) = &bytes {
            if let Ok(m) = exif::load_metadata_from_bytes(path, bytes) {
                has_gps = exif::read_gps(&m).is_some();
                has_date = exif::read_capture_time(&m).is_some();
                for t in exif::list_tags(&m) {
                    props.push((t.name.to_ascii_lowercase(), t.value.to_ascii_lowercase()));
                }
            }
            if let Some(pkt) = xmp::extract_packet_bytes(bytes) {
                has_xmp = true;
                if let Ok(s) = std::str::from_utf8(&pkt) {
                    props.extend(
                        xmp::read_properties(s)
                            .into_iter()
                            .map(|(n, v)| (n.to_ascii_lowercase(), v.to_ascii_lowercase())),
                    );
                }
            }
            props.extend(
                iptc::read_properties(bytes)
                    .into_iter()
                    .map(|(n, v)| (n.to_ascii_lowercase(), v.to_ascii_lowercase())),
            );
        }

        Self {
            props,
            has_gps,
            has_date,
            has_xmp,
        }
    }

    /// 是否存在名称包含 `name` 的标签（跨 EXIF / XMP / IPTC）。`name` 需为小写。
    pub fn has_tag_name(&self, name_lower: &str) -> bool {
        self.props.iter().any(|(n, _)| n.contains(name_lower))
    }
}

impl WhereExpr {
    pub fn matches(&self, facts: &FileFacts) -> bool {
        if self.any {
            self.conditions.iter().any(|c| c.matches(facts))
        } else {
            self.conditions.iter().all(|c| c.matches(facts))
        }
    }

    /// 那些「标签名压根不存在时会匹配所有文件」的条件里用到的标签名。
    ///
    /// `!=`、`!~` 与 `no:名称` 都是取反语义：匹配不到任何标签就没有值可比，
    /// 取反后对每个文件都为真。于是 `camera!=Canon`（根本没有 camera 这个标签）
    /// 会选中全部文件——配上 `strip` 就是一个笔误把「一部分」变成「全部」。
    /// 调用方应当拿这些名字去确认它们至少在某个文件里存在，见 [`FileFacts::has_tag_name`]。
    pub fn negative_tag_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.conditions {
            let name = match c {
                Condition::Tag {
                    name,
                    op: Op::Ne | Op::NotContains,
                    ..
                } => name,
                Condition::TagPresence {
                    name,
                    present: false,
                } => name,
                _ => continue,
            };
            if !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                out.push(name.clone());
            }
        }
        out
    }
}

fn strip_prefix_ci(s: &str, prefix: &str) -> Option<String> {
    // `get` 而不是索引切片：直接切会在多字节字符中间 panic（如 `--where 😀`）。
    let head = s.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(s[prefix.len()..].trim().to_string())
    } else {
        None
    }
}

impl Condition {
    /// 该文件是否满足条件。读不出来的文件按“不满足存在性”处理。
    pub fn matches(&self, facts: &FileFacts) -> bool {
        match self {
            Condition::HasGps => facts.has_gps,
            Condition::NoGps => !facts.has_gps,
            Condition::HasDate => facts.has_date,
            Condition::NoDate => !facts.has_date,
            Condition::HasXmp => facts.has_xmp,
            Condition::NoXmp => !facts.has_xmp,
            Condition::TagPresence { name, present } => {
                facts.has_tag_name(&name.to_ascii_lowercase()) == *present
            }
            Condition::Tag { name, op, value } => eval_tag(facts, name, op, value),
        }
    }
}

fn eval_tag(facts: &FileFacts, name: &str, op: &Op, value: &str) -> bool {
    let name_l = name.to_ascii_lowercase();
    let value_l = value.to_ascii_lowercase();
    let mut eq = false;
    let mut contains = false;
    for (n, v) in &facts.props {
        if !n.contains(&name_l) {
            continue;
        }
        if *v == value_l {
            eq = true;
        }
        if v.contains(&value_l) {
            contains = true;
        }
    }
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

    #[test]
    fn combine_and_or() {
        let and = parse_expr("no-gps && make=Canon").unwrap();
        assert_eq!(and.conditions.len(), 2);
        assert!(!and.any);

        let or = parse_expr("no-gps || no-date").unwrap();
        assert_eq!(or.conditions.len(), 2);
        assert!(or.any);

        // 单条件仍可用
        let single = parse_expr("has-gps").unwrap();
        assert_eq!(single.conditions.len(), 1);
    }
}

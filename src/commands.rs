//! 各子命令的处理逻辑。

use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone, Utc};
use little_exif::exif_tag::ExifTag;

use crate::cli::{
    ApplyArgs, CompletionsArgs, CopyArgs, GeotagArgs, GpsArgs, IptcArgs, RenameArgs, ReportArgs,
    RestoreArgs, RotateArgs, SetArgs, ShowArgs, StripArgs, TargetArgs, TimeArgs, VerifyArgs,
    WriteArgs, XmpArgs,
};
use crate::exif::{self, RotateOp, TagSelection, WriteOpts};
use crate::gpx::{self, TrackPoint};
use crate::iptc::{self, IptcEdit};
use crate::namedate;
use crate::scan;
use crate::timeop::{self, Delta, parse_datetime, parse_delta};
use crate::xmp::{self, XmpEdit, XmpValue};

// ============================ 通用工具 ============================

/// 单个文件的处理结局。
enum Outcome {
    Changed(String),
    Skipped(String),
    Failed(String),
}

#[derive(Default)]
struct Stats {
    changed: usize,
    skipped: usize,
    failed: usize,
}

fn collect(target: &TargetArgs) -> Vec<std::path::PathBuf> {
    let ext = scan::parse_ext_set(&target.ext);
    scan::collect_files(&target.paths, &ext, target.recursive)
}

/// 按 `--where` 条件筛选文件。筛选说明写到 stderr，避免污染 show 的 JSON/CSV 输出。
fn apply_where(
    files: Vec<std::path::PathBuf>,
    expr: &Option<String>,
) -> Result<Vec<std::path::PathBuf>> {
    let Some(expr) = expr else {
        return Ok(files);
    };
    let cond = crate::whereexpr::parse_expr(expr)?;
    let total = files.len();
    let kept: Vec<_> = files.into_iter().filter(|p| cond.matches(p)).collect();
    eprintln!("筛选 --where {expr}：{}/{total} 个文件符合条件", kept.len());
    Ok(kept)
}

fn write_opts(w: &WriteArgs, preserve_fs_time: bool) -> WriteOpts {
    WriteOpts {
        dry_run: w.dry_run,
        backup: w.backup,
        preserve_fs_time,
    }
}

/// 写入前确认（预览或 -y 时直接放行）。
fn confirm_write(w: &WriteArgs) -> Result<bool> {
    if w.dry_run || w.yes {
        return Ok(true);
    }
    print!("确认执行？[y/N] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let ans = line.trim().to_ascii_lowercase();
    Ok(ans == "y" || ans == "yes")
}

fn print_outcome(path: &Path, outcome: &Outcome, verbose: bool) {
    let name = path.display();
    match outcome {
        Outcome::Changed(detail) => {
            if detail.is_empty() {
                println!("[OK]   {name}");
            } else {
                println!("[OK]   {name}");
                if verbose || !detail.is_empty() {
                    println!("         {detail}");
                }
            }
        }
        Outcome::Skipped(reason) => println!("[跳过] {name}  ({reason})"),
        Outcome::Failed(error) => println!("[失败] {name}  ({error})"),
    }
}

fn tally(stats: &mut Stats, outcome: &Outcome) {
    match outcome {
        Outcome::Changed(_) => stats.changed += 1,
        Outcome::Skipped(_) => stats.skipped += 1,
        Outcome::Failed(_) => stats.failed += 1,
    }
}

fn print_summary(stats: &Stats, total: usize, dry_run: bool) {
    println!();
    let verb = if dry_run { "将修改" } else { "已修改" };
    print!("{verb} {}", stats.changed);
    if stats.skipped > 0 {
        print!("，跳过 {}", stats.skipped);
    }
    if stats.failed > 0 {
        print!("，失败 {}", stats.failed);
    }
    println!("，共 {total} 个文件。");
    if dry_run {
        println!("（预览模式，未写入任何文件；去掉 -n/--dry-run 即可执行）");
    }
}

/// 创建进度条：仅在多文件且 stderr 是终端时显示（画在 stderr，不污染 stdout）。
fn progress_bar(len: usize) -> indicatif::ProgressBar {
    use std::io::IsTerminal;
    if len > 1 && std::io::stderr().is_terminal() {
        let pb = indicatif::ProgressBar::new(len as u64);
        pb.set_style(
            indicatif::ProgressStyle::with_template("  {bar:32} {pos}/{len} ({eta}) {msg}")
                .unwrap()
                .progress_chars("##-"),
        );
        pb
    } else {
        indicatif::ProgressBar::hidden()
    }
}

/// 并行处理一批文件，带进度条；结果按原顺序打印，随后打印汇总。
///
/// `jobs`：0=rayon 默认（CPU 核数），1=顺序，其它=指定线程数。
/// 逐文件写入互不干扰（各写各的文件、临时文件名唯一），因此并行安全。
fn run_batch<F>(files: &[std::path::PathBuf], write: &WriteArgs, process: F) -> Stats
where
    F: Fn(usize, &Path) -> Outcome + Sync,
{
    use rayon::prelude::*;

    let pb = progress_bar(files.len());
    let run = || {
        files
            .par_iter()
            .enumerate()
            .map(|(i, p)| {
                let o = process(i, p);
                pb.inc(1);
                o
            })
            .collect::<Vec<Outcome>>()
    };

    let results: Vec<Outcome> = match write.jobs {
        1 => files
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let o = process(i, p);
                pb.inc(1);
                o
            })
            .collect(),
        0 => run(),
        n => match rayon::ThreadPoolBuilder::new().num_threads(n).build() {
            Ok(pool) => pool.install(run),
            Err(_) => run(),
        },
    };
    pb.finish_and_clear();

    let mut stats = Stats::default();
    for (path, outcome) in files.iter().zip(results.iter()) {
        tally(&mut stats, outcome);
        print_outcome(path, outcome, write.verbose);
    }
    print_summary(&stats, files.len(), write.dry_run);
    stats
}

/// 解析 --tags 时间字段列表。
fn parse_time_tags(s: &str) -> Result<TagSelection> {
    let mut sel = TagSelection {
        original: false,
        digitized: false,
        modify: false,
    };
    for part in s.split(',') {
        match part.trim().to_ascii_lowercase().as_str() {
            "" => continue,
            "original" | "datetimeoriginal" | "o" => sel.original = true,
            "digitized" | "createdate" | "datetimedigitized" | "d" => sel.digitized = true,
            "modify" | "modifydate" | "datetime" | "m" => sel.modify = true,
            "all" => {
                sel.original = true;
                sel.digitized = true;
                sel.modify = true;
            }
            other => bail!("未知的时间字段 `{other}`，可用值：original,digitized,modify,all"),
        }
    }
    if !sel.any() {
        bail!("--tags 未选择任何字段");
    }
    Ok(sel)
}

// ============================ time ============================

enum TimeMode {
    Set(NaiveDateTime),
    Shift(Delta),
    Sequential {
        start: NaiveDateTime,
        interval: Delta,
    },
    FromName,
}

pub fn time(args: TimeArgs) -> Result<usize> {
    let mode = build_time_mode(&args)?;
    let sel = parse_time_tags(&args.tags)?;
    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · 修改拍摄时间");
    println!("  操作：{}", describe_time_mode(&mode));
    println!("  字段：{}", describe_tags(sel));
    println!("  文件：{} 个", files.len());
    // 规范化时区偏移（如 +8 → +08:00）
    let tz = match &args.tz {
        Some(s) => Some(parse_tz(s)?.to_string()),
        None => None,
    };
    if let Some(tz) = &tz {
        println!("  时区：写入 OffsetTime {tz}");
    }
    if args.also_file_time {
        println!("  附加：同步设置文件系统修改时间");
    }
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, !args.also_file_time);
    let stats = run_batch(&files, &args.write, |i, path| {
        process_time(
            path,
            i,
            &mode,
            sel,
            &opts,
            args.also_file_time,
            tz.as_deref(),
        )
    });
    Ok(stats.failed)
}

#[allow(clippy::too_many_arguments)]
fn process_time(
    path: &Path,
    index: usize,
    mode: &TimeMode,
    sel: TagSelection,
    opts: &WriteOpts,
    also_file_time: bool,
    tz: Option<&str>,
) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let mut metadata = match exif::load_metadata(path) {
        Ok(m) => m,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    let old = exif::read_capture_time(&metadata);

    let target = match mode {
        TimeMode::Set(t) => *t,
        TimeMode::Sequential { start, interval } => {
            match interval.scaled(index as i64).apply(*start) {
                Some(t) => t,
                None => return Outcome::Failed("时间计算溢出".into()),
            }
        }
        TimeMode::Shift(delta) => match old.as_deref().and_then(|s| parse_datetime(s).ok()) {
            Some(dt) => match delta.apply(dt) {
                Some(t) => t,
                None => return Outcome::Failed("时间计算溢出".into()),
            },
            None => return Outcome::Skipped("无可解析的原始拍摄时间，偏移模式跳过".into()),
        },
        TimeMode::FromName => {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            match namedate::extract(stem) {
                Some(t) => t,
                None => return Outcome::Skipped("文件名中未识别到日期".into()),
            }
        }
    };

    exif::apply_datetime(&mut metadata, target, sel);
    if let Some(tz) = tz {
        exif::apply_offset_time(&mut metadata, tz, sel);
    }
    if let Err(e) = exif::commit_metadata(path, &metadata, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    if also_file_time
        && !opts.dry_run
        && let Err(e) = exif::set_file_time(path, target)
    {
        return Outcome::Failed(format!("EXIF 已写入但设置文件时间失败：{e:#}"));
    }

    let old = old.unwrap_or_else(|| "(无)".into());
    Outcome::Changed(format!("{old}  ->  {}", timeop::format_exif(&target)))
}

fn build_time_mode(a: &TimeArgs) -> Result<TimeMode> {
    if let Some(s) = &a.set {
        return Ok(TimeMode::Set(
            parse_datetime(s).context("解析 --set 时间失败")?,
        ));
    }
    if let Some(s) = &a.shift {
        let d = parse_delta(s).context("解析 --shift 偏移量失败")?;
        if d.is_zero() {
            bail!("--shift 偏移量为零，无需修改");
        }
        return Ok(TimeMode::Shift(d));
    }
    if let Some(s) = &a.sequential {
        let start = parse_datetime(s).context("解析 --sequential 起始时间失败")?;
        let interval = parse_delta(&a.interval).context("解析 --interval 间隔失败")?;
        return Ok(TimeMode::Sequential { start, interval });
    }
    if a.from_name {
        return Ok(TimeMode::FromName);
    }
    bail!("必须指定一种操作：--set / --shift / --sequential / --from-name");
}

fn describe_time_mode(mode: &TimeMode) -> String {
    match mode {
        TimeMode::Set(t) => format!("设为固定时间 {}", timeop::format_exif(t)),
        TimeMode::Shift(d) => format!("在原时间上偏移 {}", describe_delta(d)),
        TimeMode::Sequential { start, interval } => {
            format!(
                "从 {} 起，每张递增 {}",
                timeop::format_exif(start),
                describe_delta(interval)
            )
        }
        TimeMode::FromName => "从文件名提取日期".into(),
    }
}

fn describe_tags(t: TagSelection) -> String {
    let mut v = Vec::new();
    if t.original {
        v.push("拍摄(DateTimeOriginal)");
    }
    if t.digitized {
        v.push("数字化(CreateDate)");
    }
    if t.modify {
        v.push("修改(ModifyDate)");
    }
    v.join("、")
}

fn describe_delta(d: &Delta) -> String {
    let negative = d.months < 0 || d.days < 0 || d.seconds < 0;
    let sign = if negative { "-" } else { "+" };
    let (m, days, secs) = (d.months.abs(), d.days.abs(), d.seconds.abs());
    let mut parts = Vec::new();
    if m >= 12 {
        parts.push(format!("{}年", m / 12));
    }
    if m % 12 != 0 {
        parts.push(format!("{}个月", m % 12));
    }
    if days != 0 {
        parts.push(format!("{days}天"));
    }
    let (h, mi, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h != 0 {
        parts.push(format!("{h}小时"));
    }
    if mi != 0 {
        parts.push(format!("{mi}分"));
    }
    if s != 0 {
        parts.push(format!("{s}秒"));
    }
    if parts.is_empty() {
        return "0".into();
    }
    format!("{sign}{}", parts.join(""))
}

// ============================ show ============================

pub fn show(args: ShowArgs) -> Result<usize> {
    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }
    let filter = args.filter.as_deref().map(|s| s.to_ascii_lowercase());

    if args.json {
        return show_json(&files, filter.as_deref());
    }
    if args.csv {
        return show_csv(&files, filter.as_deref());
    }

    let mut failed = 0;
    for (idx, path) in files.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        println!("=== {} ===", path.display());
        if let Some(hint) = exif::unsupported_hint(path) {
            println!("  {hint}");
            continue;
        }
        let sidecar = filter_props(read_sidecar_props(path), filter.as_deref());
        let metadata = match exif::load_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                // 图片本身读不了（如 RAW），但若有 sidecar .xmp 仍展示
                if sidecar.is_empty() {
                    println!("  读取失败：{e:#}");
                    failed += 1;
                } else {
                    println!("  --- XMP (sidecar) ---");
                    let w = sidecar
                        .iter()
                        .map(|(k, _)| k.len())
                        .max()
                        .unwrap_or(0)
                        .min(28);
                    for (k, v) in &sidecar {
                        println!("  {k:<w$}  {v}");
                    }
                }
                continue;
            }
        };

        if let Some(fix) = exif::read_gps(&metadata) {
            let alt = fix
                .alt
                .map(|a| format!("，海拔 {a:.1}m"))
                .unwrap_or_default();
            println!("  位置：{:.6}, {:.6}{alt}", fix.lat, fix.lon);
        }

        let tags = exif::list_tags(&metadata);
        let shown: Vec<_> = tags
            .iter()
            .filter(|t| match &filter {
                Some(f) => t.name.to_ascii_lowercase().contains(f),
                None => true,
            })
            .collect();

        let xmp = filter_props(read_xmp_props(path), filter.as_deref());
        let iptc = filter_props(read_iptc_props(path), filter.as_deref());

        if shown.is_empty() && xmp.is_empty() && iptc.is_empty() && sidecar.is_empty() {
            println!("  (无匹配的元数据)");
            continue;
        }
        let width = shown
            .iter()
            .map(|t| t.name.len())
            .chain(xmp.iter().map(|(k, _)| k.len()))
            .chain(iptc.iter().map(|(k, _)| k.len()))
            .chain(sidecar.iter().map(|(k, _)| k.len()))
            .max()
            .unwrap_or(0)
            .min(28);
        for t in shown {
            println!("  {:<width$}  {}", t.name, t.value, width = width);
        }
        if !xmp.is_empty() {
            println!("  --- XMP ---");
            for (k, v) in &xmp {
                println!("  {:<width$}  {}", k, v, width = width);
            }
        }
        if !iptc.is_empty() {
            println!("  --- IPTC ---");
            for (k, v) in &iptc {
                println!("  {:<width$}  {}", k, v, width = width);
            }
        }
        if !sidecar.is_empty() {
            println!("  --- XMP (sidecar) ---");
            for (k, v) in &sidecar {
                println!("  {:<width$}  {}", k, v, width = width);
            }
        }
    }
    Ok(failed)
}

fn show_json(files: &[std::path::PathBuf], filter: Option<&str>) -> Result<usize> {
    let mut failed = 0;
    let mut out = String::from("[\n");
    let mut first = true;
    for path in files {
        if exif::unsupported_hint(path).is_some() {
            continue;
        }
        let metadata = match exif::load_metadata(path) {
            Ok(m) => m,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        if !first {
            out.push_str(",\n");
        }
        first = false;
        out.push_str("  {\n");
        out.push_str(&format!(
            "    \"file\": \"{}\",\n",
            json_escape(&path.display().to_string())
        ));
        if let Some(fix) = exif::read_gps(&metadata) {
            out.push_str(&format!(
                "    \"latitude\": {:.6},\n    \"longitude\": {:.6},\n",
                fix.lat, fix.lon
            ));
        }
        out.push_str("    \"tags\": [\n");
        let tags = exif::list_tags(&metadata);
        let shown: Vec<_> = tags
            .iter()
            .filter(|t| match filter {
                Some(f) => t.name.to_ascii_lowercase().contains(f),
                None => true,
            })
            .collect();
        for (i, t) in shown.iter().enumerate() {
            let comma = if i + 1 < shown.len() { "," } else { "" };
            out.push_str(&format!(
                "      {{\"group\": \"{}\", \"name\": \"{}\", \"hex\": \"0x{:04X}\", \"value\": \"{}\"}}{}\n",
                t.group,
                json_escape(&t.name),
                t.hex,
                json_escape(&t.value),
                comma
            ));
        }
        out.push_str("    ]");

        let xmp = filter_props(read_xmp_props(path), filter);
        if !xmp.is_empty() {
            out.push_str(",\n    \"xmp\": {\n");
            for (i, (k, v)) in xmp.iter().enumerate() {
                let comma = if i + 1 < xmp.len() { "," } else { "" };
                out.push_str(&format!(
                    "      \"{}\": \"{}\"{}\n",
                    json_escape(k),
                    json_escape(v),
                    comma
                ));
            }
            out.push_str("    }");
        }
        let iptc = filter_props(read_iptc_props(path), filter);
        if !iptc.is_empty() {
            out.push_str(",\n    \"iptc\": {\n");
            for (i, (k, v)) in iptc.iter().enumerate() {
                let comma = if i + 1 < iptc.len() { "," } else { "" };
                out.push_str(&format!(
                    "      \"{}\": \"{}\"{}\n",
                    json_escape(k),
                    json_escape(v),
                    comma
                ));
            }
            out.push_str("    }");
        }
        out.push_str("\n  }");
    }
    out.push_str("\n]");
    println!("{out}");
    Ok(failed)
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn show_csv(files: &[std::path::PathBuf], filter: Option<&str>) -> Result<usize> {
    let mut failed = 0;
    println!("file,group,name,hex,value");
    for path in files {
        if exif::unsupported_hint(path).is_some() {
            continue;
        }
        let metadata = match exif::load_metadata(path) {
            Ok(m) => m,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let file = path.display().to_string();
        for t in exif::list_tags(&metadata) {
            if let Some(f) = filter
                && !t.name.to_ascii_lowercase().contains(f)
            {
                continue;
            }
            println!(
                "{},{},{},0x{:04X},{}",
                csv_field(&file),
                csv_field(&t.group),
                csv_field(&t.name),
                t.hex,
                csv_field(&t.value)
            );
        }
        for (k, v) in filter_props(read_xmp_props(path), filter) {
            println!(
                "{},XMP,{},,{}",
                csv_field(&file),
                csv_field(&k),
                csv_field(&v)
            );
        }
        for (k, v) in filter_props(read_iptc_props(path), filter) {
            println!(
                "{},IPTC,{},,{}",
                csv_field(&file),
                csv_field(&k),
                csv_field(&v)
            );
        }
    }
    Ok(failed)
}

/// 读取文件的 XMP 属性（非 JPEG 或无 XMP 时返回空）。
fn read_xmp_props(path: &Path) -> Vec<(String, String)> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    match crate::xmp::extract_packet_bytes(&bytes) {
        Some(packet) => match std::str::from_utf8(&packet) {
            Ok(s) => crate::xmp::read_properties(s),
            Err(_) => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn filter_props(props: Vec<(String, String)>, filter: Option<&str>) -> Vec<(String, String)> {
    match filter {
        Some(f) => props
            .into_iter()
            .filter(|(k, _)| k.to_ascii_lowercase().contains(f))
            .collect(),
        None => props,
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ============================ rotate ============================

pub fn rotate(args: RotateArgs) -> Result<usize> {
    let op = if args.cw {
        RotateOp::Cw
    } else if args.ccw {
        RotateOp::Ccw
    } else if args.r180 {
        RotateOp::Rot180
    } else if args.flip_h {
        RotateOp::FlipH
    } else if args.flip_v {
        RotateOp::FlipV
    } else {
        RotateOp::Reset
    };

    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · 无损旋转标记");
    println!("  操作：{}", describe_rotate(op));
    println!("  文件：{} 个", files.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let stats = run_batch(&files, &args.write, |_, path| {
        process_rotate(path, op, &opts)
    });
    Ok(stats.failed)
}

fn process_rotate(path: &Path, op: RotateOp, opts: &WriteOpts) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let mut metadata = match exif::load_metadata(path) {
        Ok(m) => m,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    let current = exif::read_orientation(&metadata);
    let next = exif::compose_orientation(current, op);
    if next == current {
        return Outcome::Skipped(format!(
            "方向已是「{}」，无需修改",
            exif::orientation_desc(current)
        ));
    }
    metadata.set_tag(little_exif::exif_tag::ExifTag::Orientation(vec![next]));
    if let Err(e) = exif::commit_metadata(path, &metadata, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    Outcome::Changed(format!(
        "{}({}) -> {}({})",
        exif::orientation_desc(current),
        current,
        exif::orientation_desc(next),
        next
    ))
}

fn describe_rotate(op: RotateOp) -> &'static str {
    match op {
        RotateOp::Cw => "顺时针 90°",
        RotateOp::Ccw => "逆时针 90°",
        RotateOp::Rot180 => "旋转 180°",
        RotateOp::FlipH => "水平镜像",
        RotateOp::FlipV => "垂直镜像",
        RotateOp::Reset => "重置为正常",
    }
}

// ============================ copy ============================

pub fn copy(args: CopyArgs) -> Result<usize> {
    // 默认复制全部
    let (all, time, gps) = if !args.all && !args.time && !args.gps {
        (true, false, false)
    } else {
        (args.all, args.time, args.gps)
    };

    let source = exif::load_metadata(&args.from)
        .with_context(|| format!("读取参考照片失败：{}", args.from.display()))?;

    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    let scope = if all {
        "全部可复制的元数据".to_string()
    } else {
        let mut v = Vec::new();
        if time {
            v.push("拍摄时间");
        }
        if gps {
            v.push("GPS");
        }
        v.join("、")
    };

    println!("PIC-Killer · 复制元数据");
    println!("  来源：{}", args.from.display());
    println!("  内容：{scope}");
    println!("  文件：{} 个", files.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let stats = run_batch(&files, &args.write, |_, path| {
        process_copy(path, &source, all, time, gps, &opts)
    });
    Ok(stats.failed)
}

fn process_copy(
    path: &Path,
    source: &little_exif::metadata::Metadata,
    all: bool,
    time: bool,
    gps: bool,
    opts: &WriteOpts,
) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let mut metadata = match exif::load_metadata(path) {
        Ok(m) => m,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    let n = exif::copy_tags(source, &mut metadata, all, time, gps);
    if n == 0 {
        return Outcome::Skipped("参考照片没有可复制的对应元数据".into());
    }
    if let Err(e) = exif::commit_metadata(path, &metadata, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    Outcome::Changed(format!("复制了 {n} 个标签"))
}

// ============================ rename ============================

pub fn rename(args: RenameArgs) -> Result<usize> {
    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · 按拍摄时间重命名");
    println!("  模板：{}", args.pattern);
    println!("  文件：{} 个", files.len());
    if args.dry_run {
        println!("  模式：预览（不重命名）");
    }
    let write = WriteArgs {
        backup: false,
        dry_run: args.dry_run,
        yes: args.yes,
        verbose: args.verbose,
        jobs: 1,
    };
    if !confirm_write(&write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    // 记录已占用的目标路径（磁盘上已存在的 + 本批已分配的），避免冲突
    let mut claimed: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();
    let mut stats = Stats::default();

    for path in &files {
        let outcome = plan_rename(path, &args.pattern, &mut claimed, args.dry_run);
        tally(&mut stats, &outcome);
        print_outcome(path, &outcome, args.verbose);
    }
    print_summary(&stats, files.len(), args.dry_run);
    Ok(stats.failed)
}

fn plan_rename(
    path: &Path,
    pattern: &str,
    claimed: &mut std::collections::HashSet<std::path::PathBuf>,
    dry_run: bool,
) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let metadata = match exif::load_metadata(path) {
        Ok(m) => m,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    let capture = match exif::read_capture_time(&metadata).and_then(|s| parse_datetime(&s).ok()) {
        Some(dt) => dt,
        None => return Outcome::Skipped("无可解析的拍摄时间".into()),
    };

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let stem = capture.format(pattern).to_string();
    if stem.is_empty() {
        return Outcome::Failed("模板生成的文件名为空".into());
    }

    // 解决重名：base、base_1、base_2……
    let mut target = build_target(dir, &stem, ext, None);
    let mut counter = 1;
    while target != *path && (claimed.contains(&target) || target.exists()) {
        target = build_target(dir, &stem, ext, Some(counter));
        counter += 1;
    }

    if target == *path {
        return Outcome::Skipped("文件名已符合".into());
    }

    let new_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    if !dry_run && let Err(e) = std::fs::rename(path, &target) {
        return Outcome::Failed(format!("重命名失败：{e}"));
    }
    claimed.insert(target);
    Outcome::Changed(format!("-> {new_name}"))
}

fn build_target(dir: &Path, stem: &str, ext: &str, counter: Option<u32>) -> std::path::PathBuf {
    let name = match counter {
        Some(c) => {
            if ext.is_empty() {
                format!("{stem}_{c}")
            } else {
                format!("{stem}_{c}.{ext}")
            }
        }
        None => {
            if ext.is_empty() {
                stem.to_string()
            } else {
                format!("{stem}.{ext}")
            }
        }
    };
    dir.join(name)
}

// ============================ xmp ============================

pub fn xmp(args: XmpArgs) -> Result<usize> {
    if args.clear
        && (args.title.is_some()
            || args.description.is_some()
            || !args.creator.is_empty()
            || args.rights.is_some()
            || args.rating.is_some()
            || args.label.is_some()
            || args.keywords.is_some()
            || args.city.is_some()
            || args.country.is_some()
            || !args.set.is_empty()
            || !args.remove.is_empty())
    {
        bail!("--clear 不能与其它设置/删除同时使用");
    }

    let edit = build_xmp_edit(&args)?;
    if !args.clear && edit.is_empty() {
        bail!("未指定任何 XMP 操作，用 `pic-killer xmp --help` 查看可用选项");
    }

    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · XMP 元数据");
    if args.clear {
        println!("  操作：清除整个 XMP 包");
    } else {
        if !edit.sets.is_empty() {
            let names: Vec<&str> = edit.sets.iter().map(|(q, _)| q.as_str()).collect();
            println!("  设置：{}", names.join("、"));
        }
        if !edit.removes.is_empty() {
            println!("  删除：{}", edit.removes.join("、"));
        }
    }
    println!("  文件：{} 个（仅处理 JPEG/PNG）", files.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let sidecar = args.sidecar;
    let stats = run_batch(&files, &args.write, |_, path| {
        process_xmp(path, &edit, args.clear, sidecar, &opts)
    });
    Ok(stats.failed)
}

fn process_xmp(
    path: &Path,
    edit: &XmpEdit,
    clear: bool,
    sidecar: bool,
    opts: &WriteOpts,
) -> Outcome {
    if sidecar {
        return process_xmp_sidecar(path, edit, clear, opts);
    }
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let mut bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return Outcome::Failed(format!("读取失败：{e}")),
    };
    if !xmp::supports_xmp(&bytes) {
        return Outcome::Skipped("XMP 目前仅支持 JPEG 与 PNG".into());
    }

    if clear {
        if !xmp::remove_packet(&mut bytes) {
            return Outcome::Skipped("本就没有 XMP".into());
        }
        return match exif::commit_raw(path, &bytes, opts) {
            Ok(()) => Outcome::Changed("已清除 XMP".into()),
            Err(e) => Outcome::Failed(format!("{e:#}")),
        };
    }

    let existing = xmp::extract_packet_bytes(&bytes);
    let existing_str = match &existing {
        Some(p) => match std::str::from_utf8(p) {
            Ok(s) => Some(s),
            Err(_) => return Outcome::Failed("现有 XMP 非 UTF-8，为安全起见跳过".into()),
        },
        None => None,
    };

    let packet = match xmp::apply(existing_str, edit) {
        Ok(p) => p,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    if let Err(e) = xmp::write_packet(&mut bytes, &packet) {
        return Outcome::Failed(format!("{e:#}"));
    }
    if let Err(e) = exif::commit_raw(path, &bytes, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    Outcome::Changed(format!(
        "设置 {} 项，删除 {} 项",
        edit.sets.len(),
        edit.removes.len()
    ))
}

/// sidecar 模式：只读写 `<主干>.xmp`，完全不碰原图（因此支持 RAW 等任意文件）。
fn process_xmp_sidecar(path: &Path, edit: &XmpEdit, clear: bool, opts: &WriteOpts) -> Outcome {
    let scar = xmp::sidecar_path(path);
    let name = scar
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    if clear {
        if !scar.exists() {
            return Outcome::Skipped("无 sidecar 可清除".into());
        }
        if opts.dry_run {
            return Outcome::Changed(format!("将删除 {name}"));
        }
        return match std::fs::remove_file(&scar) {
            Ok(()) => Outcome::Changed(format!("已删除 sidecar {name}")),
            Err(e) => Outcome::Failed(format!("删除 sidecar 失败：{e}")),
        };
    }

    let existing = xmp::read_sidecar(path);
    let packet = match xmp::apply(existing.as_deref(), edit) {
        Ok(p) => p,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    if opts.dry_run {
        return Outcome::Changed(format!("将写入 sidecar {name}"));
    }
    match std::fs::write(&scar, packet) {
        Ok(()) => Outcome::Changed(format!("已写入 sidecar {name}")),
        Err(e) => Outcome::Failed(format!("写入 sidecar 失败：{e}")),
    }
}

fn build_xmp_edit(a: &XmpArgs) -> Result<XmpEdit> {
    let mut edit = XmpEdit::default();
    if let Some(v) = &a.title {
        edit.sets
            .push(("dc:title".into(), XmpValue::LangAlt(v.clone())));
    }
    if let Some(v) = &a.description {
        edit.sets
            .push(("dc:description".into(), XmpValue::LangAlt(v.clone())));
    }
    if !a.creator.is_empty() {
        edit.sets
            .push(("dc:creator".into(), XmpValue::Seq(a.creator.clone())));
    }
    if let Some(v) = &a.rights {
        edit.sets
            .push(("dc:rights".into(), XmpValue::LangAlt(v.clone())));
    }
    if let Some(r) = a.rating {
        if !(0..=5).contains(&r) {
            bail!("--rating 需在 0-5 之间");
        }
        edit.sets
            .push(("xmp:Rating".into(), XmpValue::Simple(r.to_string())));
    }
    if let Some(v) = &a.label {
        edit.sets
            .push(("xmp:Label".into(), XmpValue::Simple(v.clone())));
    }
    if let Some(v) = &a.keywords {
        let items: Vec<String> = v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        edit.sets.push(("dc:subject".into(), XmpValue::Bag(items)));
    }
    if let Some(v) = &a.city {
        edit.sets
            .push(("photoshop:City".into(), XmpValue::Simple(v.clone())));
    }
    if let Some(v) = &a.country {
        edit.sets
            .push(("photoshop:Country".into(), XmpValue::Simple(v.clone())));
    }
    for kv in &a.set {
        let (qname, val) = kv
            .split_once('=')
            .with_context(|| format!("--set 格式应为 前缀:名称=值，收到 `{kv}`"))?;
        let qname = qname.trim();
        let prefix = qname.split(':').next().unwrap_or("");
        if xmp::namespace_uri(prefix).is_none() {
            bail!("未知的命名空间前缀 `{prefix}`（支持 dc/xmp/photoshop/lr 等）");
        }
        edit.sets
            .push((qname.to_string(), XmpValue::Simple(val.to_string())));
    }
    for name in &a.remove {
        edit.removes.push(name.trim().to_string());
    }
    Ok(edit)
}

// ============================ iptc ============================

pub fn iptc(args: IptcArgs) -> Result<usize> {
    let has_edits = args.title.is_some()
        || args.description.is_some()
        || args.keywords.is_some()
        || !args.creator.is_empty()
        || args.headline.is_some()
        || args.city.is_some()
        || args.state.is_some()
        || args.country.is_some()
        || args.copyright.is_some()
        || args.credit.is_some()
        || args.source.is_some()
        || args.instructions.is_some()
        || !args.set.is_empty()
        || !args.remove.is_empty();

    if args.clear && has_edits {
        bail!("--clear 不能与其它设置/删除同时使用");
    }
    let edit = build_iptc_edit(&args)?;
    if !args.clear && edit.is_empty() {
        bail!("未指定任何 IPTC 操作，用 `pic-killer iptc --help` 查看可用选项");
    }

    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · IPTC-IIM 元数据");
    if args.clear {
        println!("  操作：清除整个 IPTC 块");
    } else {
        if !edit.sets.is_empty() {
            let names: Vec<String> = edit
                .sets
                .iter()
                .map(|(r, n, _)| iptc::field_name(*r, *n))
                .collect();
            println!("  设置：{}", names.join("、"));
        }
        if !edit.removes.is_empty() {
            let names: Vec<String> = edit
                .removes
                .iter()
                .map(|(r, n)| iptc::field_name(*r, *n))
                .collect();
            println!("  删除：{}", names.join("、"));
        }
    }
    println!("  文件：{} 个（仅处理 JPEG）", files.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let stats = run_batch(&files, &args.write, |_, path| {
        process_iptc(path, &edit, args.clear, &opts)
    });
    Ok(stats.failed)
}

fn process_iptc(path: &Path, edit: &IptcEdit, clear: bool, opts: &WriteOpts) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let mut bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return Outcome::Failed(format!("读取失败：{e}")),
    };
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Outcome::Skipped("IPTC-IIM 仅支持 JPEG".into());
    }

    if clear {
        if !iptc::remove_jpeg_iptc(&mut bytes) {
            return Outcome::Skipped("本就没有 IPTC".into());
        }
        return match exif::commit_raw(path, &bytes, opts) {
            Ok(()) => Outcome::Changed("已清除 IPTC".into()),
            Err(e) => Outcome::Failed(format!("{e:#}")),
        };
    }

    let existing = iptc::read_datasets(&bytes);
    let datasets = iptc::apply(&existing, edit);
    if let Err(e) = iptc::set_jpeg_iptc(&mut bytes, &datasets) {
        return Outcome::Failed(format!("{e:#}"));
    }
    if let Err(e) = exif::commit_raw(path, &bytes, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    Outcome::Changed(format!(
        "设置 {} 项，删除 {} 项",
        edit.sets.len(),
        edit.removes.len()
    ))
}

fn build_iptc_edit(a: &IptcArgs) -> Result<IptcEdit> {
    let mut edit = IptcEdit::default();
    let mut single = |field: &Option<String>, r: u8, n: u8| {
        if let Some(v) = field {
            edit.sets.push((r, n, vec![v.clone()]));
        }
    };
    single(&a.title, 2, 5);
    single(&a.description, 2, 120);
    single(&a.headline, 2, 105);
    single(&a.city, 2, 90);
    single(&a.state, 2, 95);
    single(&a.country, 2, 101);
    single(&a.copyright, 2, 116);
    single(&a.credit, 2, 110);
    single(&a.source, 2, 115);
    single(&a.instructions, 2, 40);

    if let Some(kw) = &a.keywords {
        let items: Vec<String> = kw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        edit.sets.push((2, 25, items));
    }
    if !a.creator.is_empty() {
        edit.sets.push((2, 80, a.creator.clone()));
    }

    for kv in &a.set {
        let (name, val) = kv
            .split_once('=')
            .with_context(|| format!("--set 格式应为 字段=值，收到 `{kv}`"))?;
        let (r, n) =
            iptc::resolve_field(name).with_context(|| format!("未知的 IPTC 字段 `{name}`"))?;
        edit.sets.push((r, n, vec![val.to_string()]));
    }
    for name in &a.remove {
        let (r, n) =
            iptc::resolve_field(name).with_context(|| format!("未知的 IPTC 字段 `{name}`"))?;
        edit.removes.push((r, n));
    }
    Ok(edit)
}

fn read_iptc_props(path: &Path) -> Vec<(String, String)> {
    match std::fs::read(path) {
        Ok(bytes) => iptc::read_properties(&bytes),
        Err(_) => Vec::new(),
    }
}

/// 读取 sidecar `.xmp` 里的属性（不存在则空）。
fn read_sidecar_props(path: &Path) -> Vec<(String, String)> {
    match xmp::read_sidecar(path) {
        Some(pkt) => xmp::read_properties(&pkt),
        None => Vec::new(),
    }
}

// ============================ restore ============================

pub fn restore(args: RestoreArgs) -> Result<usize> {
    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }
    let backups = files
        .iter()
        .filter(|p| exif::backup_path(p).exists())
        .count();

    println!("PIC-Killer · 从备份还原");
    println!("  找到 .bak：{backups} / {} 个文件", files.len());
    println!(
        "  备份处理：{}",
        if args.keep_backup {
            "还原后保留 .bak"
        } else {
            "还原后移除 .bak"
        }
    );
    if args.dry_run {
        println!("  模式：预览（不操作）");
    }
    if backups == 0 {
        println!("没有找到任何 .bak 备份，无需还原。");
        return Ok(0);
    }

    let write = WriteArgs {
        backup: false,
        dry_run: args.dry_run,
        yes: args.yes,
        verbose: args.verbose,
        jobs: 1,
    };
    if !confirm_write(&write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let mut stats = Stats::default();
    for path in &files {
        let outcome = process_restore(path, args.keep_backup, args.dry_run);
        tally(&mut stats, &outcome);
        print_outcome(path, &outcome, args.verbose);
    }

    println!();
    let verb = if args.dry_run {
        "将还原"
    } else {
        "已还原"
    };
    print!("{verb} {}", stats.changed);
    if stats.skipped > 0 {
        print!("，跳过 {}", stats.skipped);
    }
    if stats.failed > 0 {
        print!("，失败 {}", stats.failed);
    }
    println!("，共 {} 个文件。", files.len());
    Ok(stats.failed)
}

fn process_restore(path: &Path, keep_backup: bool, dry_run: bool) -> Outcome {
    let bak = exif::backup_path(path);
    if !bak.exists() {
        return Outcome::Skipped("无 .bak 备份".into());
    }
    if dry_run {
        return Outcome::Changed("将从 .bak 还原".into());
    }
    let result = if keep_backup {
        std::fs::copy(&bak, path).map(|_| ())
    } else {
        std::fs::rename(&bak, path)
    };
    match result {
        Ok(()) => Outcome::Changed(
            if keep_backup {
                "已从备份还原（保留 .bak）"
            } else {
                "已从备份还原（移除 .bak）"
            }
            .into(),
        ),
        Err(e) => Outcome::Failed(format!("还原失败：{e}")),
    }
}

// ============================ geotag ============================

pub fn geotag(args: GeotagArgs) -> Result<usize> {
    let points = gpx::parse(&args.gpx)?;
    if points.is_empty() {
        bail!("GPX 中没有带时间戳的轨迹点：{}", args.gpx.display());
    }
    let tz = args.tz.as_deref().map(parse_tz).transpose()?;
    let offset = args.offset.as_deref().map(parse_delta).transpose()?;

    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · GPX 地理标记");
    println!("  轨迹：{} 个点（{}）", points.len(), args.gpx.display());
    println!("  时区：{}", args.tz.as_deref().unwrap_or("系统本地时区"));
    if let Some(o) = &args.offset {
        println!("  时间偏移：{o}");
    }
    println!("  最大间隔：{} 秒", args.max_gap);
    println!("  文件：{} 个", files.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let stats = run_batch(&files, &args.write, |_, path| {
        process_geotag(path, &points, tz, offset, args.max_gap, &opts)
    });
    Ok(stats.failed)
}

fn process_geotag(
    path: &Path,
    points: &[TrackPoint],
    tz: Option<FixedOffset>,
    offset: Option<Delta>,
    max_gap: i64,
    opts: &WriteOpts,
) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let mut metadata = match exif::load_metadata(path) {
        Ok(m) => m,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    let capture = match exif::read_capture_time(&metadata).and_then(|s| parse_datetime(&s).ok()) {
        Some(dt) => dt,
        None => return Outcome::Skipped("无拍摄时间，无法地理标记".into()),
    };
    let adjusted = match offset {
        Some(d) => match d.apply(capture) {
            Some(x) => x,
            None => return Outcome::Failed("时间偏移溢出".into()),
        },
        None => capture,
    };
    let utc = match photo_to_utc(adjusted, tz) {
        Some(u) => u,
        None => return Outcome::Skipped("拍摄时间无法解释为有效时刻（夏令时空档？）".into()),
    };
    let (lat, lon, ele) = match gpx::locate(points, utc, max_gap) {
        Some(x) => x,
        None => return Outcome::Skipped("轨迹中无匹配时段（超出 --max-gap）".into()),
    };
    for tag in exif::gps_tags(lat, lon, ele) {
        metadata.set_tag(tag);
    }
    if let Err(e) = exif::commit_metadata(path, &metadata, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    Outcome::Changed(format!("{lat:.6}, {lon:.6}"))
}

/// 把（相机本地时区的）拍摄时间转换成 UTC。tz 为 None 时用系统本地时区。
fn photo_to_utc(naive: NaiveDateTime, tz: Option<FixedOffset>) -> Option<DateTime<Utc>> {
    match tz {
        Some(off) => off
            .from_local_datetime(&naive)
            .single()
            .map(|dt| dt.with_timezone(&Utc)),
        None => Local
            .from_local_datetime(&naive)
            .single()
            .map(|dt| dt.with_timezone(&Utc)),
    }
}

/// 解析时区偏移，如 `+08:00`、`-05:00`、`+0800`、`+8`、`Z`。
fn parse_tz(s: &str) -> Result<FixedOffset> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("z") || s.eq_ignore_ascii_case("utc") {
        return Ok(FixedOffset::east_opt(0).expect("0 偏移合法"));
    }
    let (sign, rest) = if let Some(r) = s.strip_prefix('-') {
        (-1, r)
    } else {
        (1, s.strip_prefix('+').unwrap_or(s))
    };
    let (h, m) = if let Some((h, m)) = rest.split_once(':') {
        (h.to_string(), m.to_string())
    } else if rest.len() == 4 {
        (rest[..2].to_string(), rest[2..].to_string())
    } else {
        (rest.to_string(), "0".to_string())
    };
    let hh: i32 = h.parse().map_err(|_| anyhow::anyhow!("无效时区 `{s}`"))?;
    let mm: i32 = m.parse().map_err(|_| anyhow::anyhow!("无效时区 `{s}`"))?;
    let secs = sign * (hh * 3600 + mm * 60);
    FixedOffset::east_opt(secs).context("时区偏移超出范围")
}

// ============================ apply ============================

pub fn apply(args: ApplyArgs) -> Result<usize> {
    use std::collections::HashMap;
    use std::path::PathBuf;

    let raw = std::fs::read_to_string(&args.from)
        .map_err(|e| anyhow::anyhow!("读取 CSV 失败：{}：{e}", args.from.display()))?;
    // 去掉 Excel/记事本常见的 UTF-8 BOM，否则表头首字段会带上 \u{feff}
    let text = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    // 解析 CSV，按文件分组（保持首次出现的顺序）
    let mut order: Vec<PathBuf> = Vec::new();
    let mut edits: HashMap<PathBuf, Vec<(String, String)>> = HashMap::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let cols = parse_csv_line(line);
        if cols.len() < 3 {
            bail!("CSV 第 {} 行需要 file,field,value 三列：{line}", idx + 1);
        }
        let (file, field, value) = (cols[0].trim(), cols[1].trim(), cols[2].trim());
        // 跳过表头
        if idx == 0 && file.eq_ignore_ascii_case("file") && field.eq_ignore_ascii_case("field") {
            continue;
        }
        if file.is_empty() || field.is_empty() {
            continue;
        }
        let path = PathBuf::from(file);
        if !edits.contains_key(&path) {
            order.push(path.clone());
        }
        edits
            .entry(path)
            .or_default()
            .push((field.to_string(), value.to_string()));
    }

    if order.is_empty() {
        println!("CSV 中没有可应用的记录。");
        return Ok(0);
    }

    let total: usize = edits.values().map(Vec::len).sum();
    println!("PIC-Killer · 从 CSV 导入元数据");
    println!("  来源：{}", args.from.display());
    println!("  文件：{} 个，共 {total} 条字段", order.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let stats = run_batch(&order, &args.write, |_, path| match edits.get(path) {
        Some(fields) => process_apply(path, fields, &opts),
        None => Outcome::Skipped("无字段".into()),
    });
    Ok(stats.failed)
}

fn process_apply(path: &Path, fields: &[(String, String)], opts: &WriteOpts) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }

    // 按目标元数据体系分流：EXIF / XMP(xmp: 前缀) / IPTC(iptc: 前缀)
    let mut exif_fields: Vec<(&str, &str)> = Vec::new();
    let mut xmp_edit = XmpEdit::default();
    let mut iptc_edit = IptcEdit::default();
    for (field, value) in fields {
        let fl = field.to_ascii_lowercase();
        if fl.starts_with("xmp:") {
            let (qname, val) = resolve_xmp_field(&field[4..], value);
            xmp_edit.sets.push((qname, val));
        } else if let Some(name) = fl.strip_prefix("iptc:") {
            match iptc::resolve_field(name) {
                Some((r, n)) => iptc_edit.sets.push((r, n, vec![value.clone()])),
                None => return Outcome::Failed(format!("未知 IPTC 字段 `{field}`")),
            }
        } else {
            exif_fields.push((field, value));
        }
    }

    // 1) EXIF
    if !exif_fields.is_empty() {
        let mut metadata = match exif::load_metadata(path) {
            Ok(m) => m,
            Err(e) => return Outcome::Failed(format!("{e:#}")),
        };
        for (field, value) in &exif_fields {
            if let Err(e) = apply_field(&mut metadata, field, value) {
                return Outcome::Failed(format!("{e:#}"));
            }
        }
        if let Err(e) = exif::commit_metadata(path, &metadata, opts) {
            return Outcome::Failed(format!("{e:#}"));
        }
    }

    // 2) XMP（JPEG/PNG）
    if !xmp_edit.is_empty()
        && let Err(e) = apply_xmp_edit(path, &xmp_edit, opts)
    {
        return Outcome::Failed(format!("{e:#}"));
    }

    // 3) IPTC（JPEG）
    if !iptc_edit.is_empty()
        && let Err(e) = apply_iptc_edit(path, &iptc_edit, opts)
    {
        return Outcome::Failed(format!("{e:#}"));
    }

    Outcome::Changed(format!("应用了 {} 个字段", fields.len()))
}

/// 把 XMP 编辑写入文件（JPEG/PNG 段手术 + 原子落盘）。
fn apply_xmp_edit(path: &Path, edit: &XmpEdit, opts: &WriteOpts) -> Result<()> {
    let mut bytes = std::fs::read(path)?;
    if !xmp::supports_xmp(&bytes) {
        bail!("XMP 仅支持 JPEG/PNG");
    }
    let existing = xmp::extract_packet_bytes(&bytes);
    let existing_str = match &existing {
        Some(p) => Some(std::str::from_utf8(p).map_err(|_| anyhow::anyhow!("现有 XMP 非 UTF-8"))?),
        None => None,
    };
    let packet = xmp::apply(existing_str, edit)?;
    xmp::write_packet(&mut bytes, &packet)?;
    exif::commit_raw(path, &bytes, opts)
}

/// 把 IPTC 编辑写入文件（JPEG APP13）。
fn apply_iptc_edit(path: &Path, edit: &IptcEdit, opts: &WriteOpts) -> Result<()> {
    let mut bytes = std::fs::read(path)?;
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        bail!("IPTC 仅支持 JPEG");
    }
    let datasets = iptc::apply(&iptc::read_datasets(&bytes), edit);
    iptc::set_jpeg_iptc(&mut bytes, &datasets)?;
    exif::commit_raw(path, &bytes, opts)
}

/// 把 `xmp:` 之后的字段名解析成 (限定名, 值)。既支持完整限定名（含 `:`），
/// 也支持 title/rating/keywords 等友好简写。
fn resolve_xmp_field(name: &str, value: &str) -> (String, XmpValue) {
    // 完整限定名（如 dc:title、photoshop:City）
    if name.contains(':') {
        let v = match name.to_ascii_lowercase().as_str() {
            "dc:title" | "dc:description" | "dc:rights" => XmpValue::LangAlt(value.to_string()),
            "dc:creator" => XmpValue::Seq(split_multi(value)),
            "dc:subject" => XmpValue::Bag(split_multi(value)),
            _ => XmpValue::Simple(value.to_string()),
        };
        return (name.to_string(), v);
    }
    match name.to_ascii_lowercase().as_str() {
        "title" => ("dc:title".into(), XmpValue::LangAlt(value.into())),
        "description" | "caption" => ("dc:description".into(), XmpValue::LangAlt(value.into())),
        "creator" | "author" => ("dc:creator".into(), XmpValue::Seq(split_multi(value))),
        "rights" | "copyright" => ("dc:rights".into(), XmpValue::LangAlt(value.into())),
        "rating" => ("xmp:Rating".into(), XmpValue::Simple(value.into())),
        "label" => ("xmp:Label".into(), XmpValue::Simple(value.into())),
        "keywords" | "subject" => ("dc:subject".into(), XmpValue::Bag(split_multi(value))),
        "city" => ("photoshop:City".into(), XmpValue::Simple(value.into())),
        "country" => ("photoshop:Country".into(), XmpValue::Simple(value.into())),
        other => (format!("dc:{other}"), XmpValue::Simple(value.into())),
    }
}

fn split_multi(s: &str) -> Vec<String> {
    s.split([';', '|'])
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// 把 CSV 的一个 (字段, 值) 应用到 EXIF 元数据上。
fn apply_field(
    metadata: &mut little_exif::metadata::Metadata,
    field: &str,
    value: &str,
) -> Result<()> {
    let f = field
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "");
    match f.as_str() {
        "datetimeoriginal" | "date" | "original" => {
            metadata.set_tag(ExifTag::DateTimeOriginal(datetime_value(value)?));
        }
        "createdate" | "digitized" | "datetimedigitized" => {
            metadata.set_tag(ExifTag::CreateDate(datetime_value(value)?));
        }
        "modifydate" | "modify" | "datetime" => {
            metadata.set_tag(ExifTag::ModifyDate(datetime_value(value)?));
        }
        "alldates" | "all" => {
            let v = datetime_value(value)?;
            metadata.set_tag(ExifTag::DateTimeOriginal(v.clone()));
            metadata.set_tag(ExifTag::CreateDate(v.clone()));
            metadata.set_tag(ExifTag::ModifyDate(v));
        }
        "gps" => {
            let (lat, lon, alt) = parse_gps(value)?;
            for tag in exif::gps_tags(lat, lon, alt) {
                metadata.set_tag(tag);
            }
        }
        "gpsclear" | "cleargps" => {
            exif::remove_gps(metadata);
        }
        "orientation" => {
            metadata.set_tag(exif::orientation_tag(value)?);
        }
        "usercomment" => {
            metadata.set_tag(exif::user_comment_tag(value));
        }
        _ => {
            if let Some(tag) = exif::string_tag(&f, value.to_string()) {
                metadata.set_tag(tag);
            } else if let Some(r) = exif::numeric_tag(&f, value) {
                metadata.set_tag(r?);
            } else {
                bail!("未知或暂不支持的字段 `{field}`（apply 目前支持 EXIF 字段）");
            }
        }
    }
    Ok(())
}

fn datetime_value(value: &str) -> Result<String> {
    Ok(timeop::format_exif(&parse_datetime(value)?))
}

fn parse_gps(value: &str) -> Result<(f64, f64, Option<f64>)> {
    let parts: Vec<&str> = value
        .split([',', ';', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 2 {
        bail!("GPS 值需为 `纬度,经度[,海拔]`：{value}");
    }
    let lat = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("纬度无效：{}", parts[0]))?;
    let lon = parts[1]
        .parse()
        .map_err(|_| anyhow::anyhow!("经度无效：{}", parts[1]))?;
    let alt = parts.get(2).and_then(|s| s.parse().ok());
    Ok((lat, lon, alt))
}

/// 解析一行 CSV，支持双引号包裹与 `""` 转义。
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => fields.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            }
        }
    }
    fields.push(cur);
    fields
}

// ============================ set ============================

pub fn set(args: SetArgs) -> Result<usize> {
    let sets = build_set_tags(&args)?;
    let removes: Vec<String> = args.remove.clone();

    if sets.is_empty() && removes.is_empty() {
        bail!("未指定任何要设置或删除的标签，用 `pic-killer set --help` 查看可用选项");
    }

    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · 设置标签");
    if !sets.is_empty() {
        let names: Vec<String> = sets.iter().map(exif::tag_name).collect();
        println!("  设置：{}", names.join("、"));
    }
    if !removes.is_empty() {
        println!("  删除：{}", removes.join("、"));
    }
    println!("  文件：{} 个", files.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let stats = run_batch(&files, &args.write, |_, path| {
        process_set(path, &sets, &removes, &opts)
    });
    Ok(stats.failed)
}

fn process_set(path: &Path, sets: &[ExifTag], removes: &[String], opts: &WriteOpts) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let mut metadata = match exif::load_metadata(path) {
        Ok(m) => m,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    for tag in sets {
        metadata.set_tag(tag.clone());
    }
    for name in removes {
        if let Err(e) = exif::remove_named(&mut metadata, name) {
            return Outcome::Failed(format!("{e:#}"));
        }
    }
    if let Err(e) = exif::commit_metadata(path, &metadata, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    Outcome::Changed(String::new())
}

fn build_set_tags(a: &SetArgs) -> Result<Vec<ExifTag>> {
    let mut tags = Vec::new();
    if let Some(v) = &a.artist {
        tags.push(ExifTag::Artist(v.clone()));
    }
    if let Some(v) = &a.copyright {
        tags.push(ExifTag::Copyright(v.clone()));
    }
    if let Some(v) = &a.description {
        tags.push(ExifTag::ImageDescription(v.clone()));
    }
    if let Some(v) = &a.software {
        tags.push(ExifTag::Software(v.clone()));
    }
    if let Some(v) = &a.make {
        tags.push(ExifTag::Make(v.clone()));
    }
    if let Some(v) = &a.model {
        tags.push(ExifTag::Model(v.clone()));
    }
    if let Some(v) = &a.lens_model {
        tags.push(ExifTag::LensModel(v.clone()));
    }
    if let Some(v) = &a.owner {
        tags.push(ExifTag::OwnerName(v.clone()));
    }
    if let Some(v) = &a.user_comment {
        tags.push(exif::user_comment_tag(v));
    }
    if let Some(v) = &a.orientation {
        tags.push(exif::orientation_tag(v)?);
    }
    for kv in &a.set_string {
        let (name, val) = kv
            .split_once('=')
            .with_context(|| format!("--set-string 格式应为 名称=值，收到 `{kv}`"))?;
        let name = name.trim();
        let tag = if let Some(t) = exif::string_tag(name, val.to_string()) {
            t
        } else if let Some(r) = exif::numeric_tag(name, val) {
            r.with_context(|| format!("字段 `{name}` 的值 `{val}` 解析失败"))?
        } else {
            bail!("`{name}` 不是受支持的可写标签（字符串或数值）");
        };
        tags.push(tag);
    }
    Ok(tags)
}

// ============================ gps ============================

pub fn gps(args: GpsArgs) -> Result<usize> {
    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · GPS 定位");
    if let (Some(lat), Some(lon)) = (args.lat, args.lon) {
        let alt = args.alt.map(|a| format!("，海拔 {a}m")).unwrap_or_default();
        println!("  设置：{lat:.6}, {lon:.6}{alt}");
    }
    if args.clear {
        println!("  删除：清除现有 GPS");
    }
    println!("  文件：{} 个", files.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let stats = run_batch(&files, &args.write, |_, path| {
        process_gps(path, &args, &opts)
    });
    Ok(stats.failed)
}

fn process_gps(path: &Path, args: &GpsArgs, opts: &WriteOpts) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    let mut metadata = match exif::load_metadata(path) {
        Ok(m) => m,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    // 先清（若 --clear），再设（若给了坐标）
    if args.clear {
        exif::remove_gps(&mut metadata);
    }
    if let (Some(lat), Some(lon)) = (args.lat, args.lon) {
        for tag in exif::gps_tags(lat, lon, args.alt) {
            metadata.set_tag(tag);
        }
    }
    if let Err(e) = exif::commit_metadata(path, &metadata, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    Outcome::Changed(String::new())
}

// ============================ strip ============================

pub fn strip(args: StripArgs) -> Result<usize> {
    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    println!("PIC-Killer · 清除元数据");
    println!(
        "  范围：{}",
        if args.gps {
            "仅 GPS 定位"
        } else {
            "全部元数据"
        }
    );
    println!("  文件：{} 个", files.len());
    if args.write.dry_run {
        println!("  模式：预览（不写入）");
    }
    if !confirm_write(&args.write)? {
        println!("已取消。");
        return Ok(0);
    }
    println!();

    let opts = write_opts(&args.write, true);
    let stats = run_batch(&files, &args.write, |_, path| {
        process_strip(path, args.gps, &opts)
    });
    Ok(stats.failed)
}

fn process_strip(path: &Path, gps_only: bool, opts: &WriteOpts) -> Outcome {
    if let Some(hint) = exif::unsupported_hint(path) {
        return Outcome::Skipped(hint);
    }
    if gps_only {
        let mut metadata = match exif::load_metadata(path) {
            Ok(m) => m,
            Err(e) => return Outcome::Failed(format!("{e:#}")),
        };
        let removed = exif::remove_gps(&mut metadata);
        if removed == 0 {
            return Outcome::Skipped("本就没有 GPS 信息".into());
        }
        if let Err(e) = exif::commit_metadata(path, &metadata, opts) {
            return Outcome::Failed(format!("{e:#}"));
        }
    } else if let Err(e) = exif::strip_all(path, opts) {
        return Outcome::Failed(format!("{e:#}"));
    }
    Outcome::Changed(String::new())
}

// ============================ report ============================

pub fn report(args: ReportArgs) -> Result<usize> {
    use std::collections::HashMap;

    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    let pb = progress_bar(files.len());
    let mut with_date = 0usize;
    let mut with_gps = 0usize;
    let mut unreadable = 0usize;
    let mut cameras: HashMap<String, usize> = HashMap::new();
    let mut min_date: Option<String> = None;
    let mut max_date: Option<String> = None;

    for path in &files {
        pb.inc(1);
        let meta = match exif::load_metadata(path) {
            Ok(m) => m,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        if let Some(d) = exif::read_capture_time(&meta) {
            with_date += 1;
            if min_date.as_ref().is_none_or(|m| &d < m) {
                min_date = Some(d.clone());
            }
            if max_date.as_ref().is_none_or(|m| &d > m) {
                max_date = Some(d);
            }
        }
        if exif::read_gps(&meta).is_some() {
            with_gps += 1;
        }
        let tags = exif::list_tags(&meta);
        let find = |n: &str| {
            tags.iter()
                .find(|t| t.name == n)
                .map(|t| t.value.clone())
                .unwrap_or_default()
        };
        let cam = format!("{} {}", find("Make"), find("Model"))
            .trim()
            .to_string();
        let cam = if cam.is_empty() {
            "(无相机信息)".to_string()
        } else {
            cam
        };
        *cameras.entry(cam).or_default() += 1;
    }
    pb.finish_and_clear();

    let total = files.len();
    println!("PIC-Killer · 元数据统计");
    println!("  共 {total} 张照片");
    println!();
    println!(
        "  拍摄时间：{with_date} 有，{} 无",
        total - with_date - unreadable
    );
    println!(
        "  GPS 定位：{with_gps} 有，{} 无",
        total - with_gps - unreadable
    );
    if unreadable > 0 {
        println!("  无法读取：{unreadable}");
    }
    if let (Some(lo), Some(hi)) = (&min_date, &max_date) {
        println!("  时间跨度：{lo}  ~  {hi}");
    }

    println!();
    println!("  相机分布：");
    let mut cams: Vec<(&String, &usize)> = cameras.iter().collect();
    cams.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let width = cams
        .iter()
        .map(|(n, _)| n.chars().count())
        .max()
        .unwrap_or(0)
        .min(32);
    for (name, count) in cams {
        println!("    {name:<width$}  {count}");
    }

    Ok(0)
}

// ============================ verify ============================

#[derive(PartialEq, Clone, Copy)]
enum Severity {
    Error,
    Warn,
}

pub fn verify(args: VerifyArgs) -> Result<usize> {
    let files = collect(&args.target);
    let files = apply_where(files, &args.target.where_expr)?;
    if files.is_empty() {
        println!("未找到符合条件的图片文件。");
        return Ok(0);
    }

    let pb = progress_bar(files.len());
    let results: Vec<Vec<(Severity, String)>> = files
        .iter()
        .map(|p| {
            let issues = check_file(p);
            pb.inc(1);
            issues
        })
        .collect();
    pb.finish_and_clear();

    let mut problems = 0usize;
    let mut warns = 0usize;
    for (path, issues) in files.iter().zip(results.iter()) {
        if issues.is_empty() {
            continue;
        }
        let has_err = issues.iter().any(|(s, _)| *s == Severity::Error);
        if has_err {
            problems += 1;
        } else {
            warns += 1;
        }
        println!(
            "{} {}",
            if has_err { "[问题]" } else { "[警告]" },
            path.display()
        );
        for (sev, msg) in issues {
            let mark = if *sev == Severity::Error { "✗" } else { "!" };
            println!("  {mark} {msg}");
        }
    }

    println!();
    let ok = files.len() - problems - warns;
    println!(
        "共 {} 张：正常 {ok}，问题 {problems}，警告 {warns}",
        files.len()
    );
    if problems == 0 && warns == 0 {
        println!("未发现元数据问题 ✓");
    }
    Ok(problems)
}

/// 检查单个文件的元数据问题，返回 (严重级, 描述) 列表。
fn check_file(path: &Path) -> Vec<(Severity, String)> {
    let mut issues = Vec::new();
    if exif::unsupported_hint(path).is_some() {
        return issues; // BMP/GIF 非损坏，不算问题
    }
    let meta = match exif::load_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            issues.push((Severity::Error, "无法读取元数据（可能损坏或不支持）".into()));
            return issues;
        }
    };
    let tags = exif::list_tags(&meta);
    let val = |n: &str| tags.iter().find(|t| t.name == n).map(|t| t.value.clone());

    let now = Local::now().naive_local();
    if let Some(d) = val("DateTimeOriginal") {
        match parse_datetime(&d) {
            Ok(dt) if dt > now => {
                issues.push((Severity::Warn, format!("拍摄时间在未来：{d}")));
            }
            Ok(_) => {}
            Err(_) => issues.push((Severity::Error, format!("拍摄时间格式异常：{d}"))),
        }
        if let Some(c) = val("CreateDate")
            && c != d
        {
            issues.push((
                Severity::Warn,
                format!("DateTimeOriginal({d}) 与 CreateDate({c}) 不一致"),
            ));
        }
    }
    if let Some(fix) = exif::read_gps(&meta)
        && (!(-90.0..=90.0).contains(&fix.lat) || !(-180.0..=180.0).contains(&fix.lon))
    {
        issues.push((
            Severity::Error,
            format!("GPS 坐标越界：{:.4}, {:.4}", fix.lat, fix.lon),
        ));
    }
    if let Some(o) = val("Orientation")
        && let Ok(n) = o.trim().parse::<i64>()
        && !(1..=8).contains(&n)
    {
        issues.push((Severity::Error, format!("方向值异常：{n}（应为 1-8）")));
    }
    issues
}

// ============================ completions ============================

pub fn completions(args: CompletionsArgs) -> Result<usize> {
    use clap::CommandFactory;
    let mut cmd = crate::cli::Cli::command();
    if args.man {
        clap_mangen::Man::new(cmd)
            .render(&mut std::io::stdout())
            .map_err(|e| anyhow::anyhow!("生成手册页失败：{e}"))?;
    } else if let Some(shell) = args.shell {
        clap_complete::generate(shell, &mut cmd, "pic-killer", &mut std::io::stdout());
    } else {
        bail!("请指定 shell（bash/zsh/fish/powershell/elvish）或加 --man");
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_plain() {
        assert_eq!(parse_csv_line("a,b,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn csv_quoted_comma() {
        // 引号内的逗号不是分隔符
        assert_eq!(
            parse_csv_line(r#"photo.jpg,gps,"30.1,120.1""#),
            vec!["photo.jpg", "gps", "30.1,120.1"]
        );
    }

    #[test]
    fn csv_escaped_quote() {
        assert_eq!(
            parse_csv_line(r#"a,b,"say ""hi"""#),
            vec!["a", "b", "say \"hi\""]
        );
    }

    #[test]
    fn csv_trailing_empty() {
        assert_eq!(parse_csv_line("a,b,"), vec!["a", "b", ""]);
    }

    #[test]
    fn parse_gps_variants() {
        assert_eq!(parse_gps("30.1,120.1").unwrap(), (30.1, 120.1, None));
        assert_eq!(
            parse_gps("30.1, 120.1, 50").unwrap(),
            (30.1, 120.1, Some(50.0))
        );
        assert!(parse_gps("30.1").is_err());
    }
}

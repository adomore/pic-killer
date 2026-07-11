//! EXIF 引擎：读取、修改、清除元数据，并以原子方式无损落盘。
//!
//! 无损保证：little_exif 只删除并重新插入元数据段，图像的压缩像素数据原封不动。
//! 本模块用「临时文件 + 原子重命名」替代库自带的原地写入，避免批量中途崩溃损坏文件，
//! 也避免新数据更短时残留尾字节。

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveDateTime;
use little_exif::endian::Endian;
use little_exif::exif_tag::ExifTag;
use little_exif::exif_tag_format::ExifTagFormat;
use little_exif::filetype::get_file_type;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;
use little_exif::rational::uR64;

use crate::timeop::format_exif;

// ============================ 写入选项与落盘 ============================

/// 写入行为选项。
#[derive(Debug, Clone, Copy)]
pub struct WriteOpts {
    pub dry_run: bool,
    pub backup: bool,
    /// 是否保留原文件系统 mtime/atime（写入会新建文件，默认恢复原时间戳）。
    pub preserve_fs_time: bool,
}

/// 对已知但没有标准元数据容器的格式（BMP、GIF 等）给出友好提示。
/// 这类格式无法无损写入元数据，返回一句建议；其余返回 None。
pub fn unsupported_hint(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let msg = match ext.as_str() {
        "bmp" => "BMP 无元数据容器，建议先转成 PNG 再处理",
        "gif" => "GIF 不支持元数据编辑，建议先转成 PNG 再处理",
        _ => return None,
    };
    Some(msg.to_string())
}

/// 读取文件元数据；若文件不含可解析的 EXIF 则返回空元数据对象。
pub fn load_metadata(path: &Path) -> Result<Metadata> {
    if let Some(hint) = unsupported_hint(path) {
        bail!("{hint}");
    }
    get_file_type(path).with_context(|| format!("不支持的文件类型：{}", path.display()))?;
    Ok(Metadata::new_from_path(path).unwrap_or_else(|_| Metadata::new()))
}

/// 把一段完整的文件字节原子写回（备份 + 保留时间戳）。所有写命令的公共落盘出口。
pub fn commit_raw(path: &Path, data: &[u8], opts: &WriteOpts) -> Result<()> {
    if opts.dry_run {
        return Ok(());
    }
    prepare_backup(path, opts)?;
    let original_times = snapshot_times(path, opts.preserve_fs_time);
    atomic_replace(path, data).with_context(|| format!("落盘失败：{}", path.display()))?;
    restore_times(path, original_times);
    Ok(())
}

/// 把（已修改的）元数据对象原子写回文件。
pub fn commit_metadata(path: &Path, metadata: &Metadata, opts: &WriteOpts) -> Result<()> {
    let file_type =
        get_file_type(path).with_context(|| format!("不支持的文件类型：{}", path.display()))?;
    if opts.dry_run {
        return Ok(());
    }
    let mut buffer = fs::read(path).with_context(|| format!("读取失败：{}", path.display()))?;
    metadata
        .write_to_vec(&mut buffer, file_type)
        .map_err(|e| anyhow::anyhow!("编码元数据失败：{e}"))?;
    commit_raw(path, &buffer, opts)
}

/// 清除文件的全部元数据（隐私清理）。
pub fn strip_all(path: &Path, opts: &WriteOpts) -> Result<()> {
    if let Some(hint) = unsupported_hint(path) {
        bail!("{hint}");
    }
    let file_type =
        get_file_type(path).with_context(|| format!("不支持的文件类型：{}", path.display()))?;
    if opts.dry_run {
        return Ok(());
    }
    let mut buffer = fs::read(path).with_context(|| format!("读取失败：{}", path.display()))?;
    Metadata::clear_metadata(&mut buffer, file_type)
        .map_err(|e| anyhow::anyhow!("清除元数据失败：{e}"))?;
    commit_raw(path, &buffer, opts)
}

/// 把文件系统的 mtime/atime 设置为指定时间（本地时区解释）。
pub fn set_file_time(path: &Path, target: NaiveDateTime) -> Result<()> {
    use chrono::{Local, TimeZone};
    let ts = Local
        .from_local_datetime(&target)
        .earliest()
        .or_else(|| Local.from_local_datetime(&target).latest())
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| target.and_utc().timestamp());
    let ft = filetime::FileTime::from_unix_time(ts, 0);
    filetime::set_file_times(path, ft, ft)
        .with_context(|| format!("设置文件时间失败：{}", path.display()))?;
    Ok(())
}

// ============================ 时间字段 ============================

/// 要写入的时间字段集合。
#[derive(Debug, Clone, Copy)]
pub struct TagSelection {
    pub original: bool,
    pub digitized: bool,
    pub modify: bool,
}

impl TagSelection {
    pub fn any(&self) -> bool {
        self.original || self.digitized || self.modify
    }
}

/// 把目标时间写入选定的时间字段。
pub fn apply_datetime(metadata: &mut Metadata, target: NaiveDateTime, sel: TagSelection) {
    let value = format_exif(&target);
    if sel.original {
        metadata.set_tag(ExifTag::DateTimeOriginal(value.clone()));
    }
    if sel.digitized {
        metadata.set_tag(ExifTag::CreateDate(value.clone()));
    }
    if sel.modify {
        metadata.set_tag(ExifTag::ModifyDate(value));
    }
}

/// 读出当前拍摄时间字符串，依次尝试 DateTimeOriginal → CreateDate → ModifyDate。
pub fn read_capture_time(metadata: &Metadata) -> Option<String> {
    let probes = [
        ExifTag::DateTimeOriginal(String::new()),
        ExifTag::CreateDate(String::new()),
        ExifTag::ModifyDate(String::new()),
    ];
    for probe in &probes {
        if let Some(tag) = metadata.get_tag(probe).next() {
            let cleaned = string_value(tag);
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }
    None
}

// ============================ 命名标签（set / remove）============================

/// 常见字符串标签：名称 → 携带值的 `ExifTag`。仅支持 STRING 类型标签。
pub fn string_tag(name: &str, value: String) -> Option<ExifTag> {
    Some(match canonical(name).as_str() {
        "artist" => ExifTag::Artist(value),
        "copyright" => ExifTag::Copyright(value),
        "description" | "imagedescription" => ExifTag::ImageDescription(value),
        "software" => ExifTag::Software(value),
        "make" => ExifTag::Make(value),
        "model" => ExifTag::Model(value),
        "lensmake" => ExifTag::LensMake(value),
        "lensmodel" => ExifTag::LensModel(value),
        "owner" | "ownername" => ExifTag::OwnerName(value),
        "serial" | "serialnumber" => ExifTag::SerialNumber(value),
        "imageid" | "imageuniqueid" => ExifTag::ImageUniqueID(value),
        _ => return None,
    })
}

/// 名称 → 一个“空壳” `ExifTag`，仅用于按 hex+group 删除标签。
pub fn tag_template(name: &str) -> Option<ExifTag> {
    // 先看是否是字符串标签
    if let Some(t) = string_tag(name, String::new()) {
        return Some(t);
    }
    Some(match canonical(name).as_str() {
        "orientation" => ExifTag::Orientation(Vec::new()),
        "usercomment" => ExifTag::UserComment(Vec::new()),
        "datetimeoriginal" | "original" => ExifTag::DateTimeOriginal(String::new()),
        "createdate" | "digitized" | "datetimedigitized" => ExifTag::CreateDate(String::new()),
        "modifydate" | "modify" | "datetime" => ExifTag::ModifyDate(String::new()),
        _ => return None,
    })
}

/// 构造 UserComment 标签（ASCII 字符集标识 + 文本字节）。
pub fn user_comment_tag(text: &str) -> ExifTag {
    let mut bytes = b"ASCII\0\0\0".to_vec();
    bytes.extend_from_slice(text.as_bytes());
    ExifTag::UserComment(bytes)
}

/// 方向说明 → Orientation 标签（EXIF 方向码 1-8）。
pub fn orientation_tag(spec: &str) -> Result<ExifTag> {
    let code: u16 = match spec.trim().to_ascii_lowercase().as_str() {
        "1" | "normal" | "top-left" | "tl" => 1,
        "2" | "mirror-h" | "flip-h" => 2,
        "3" | "180" | "rotate-180" | "bottom-right" | "br" => 3,
        "4" | "mirror-v" | "flip-v" => 4,
        "5" | "mirror-h-cw" => 5,
        "6" | "cw" | "cw90" | "90" | "rotate-cw" => 6,
        "7" | "mirror-h-ccw" => 7,
        "8" | "ccw" | "ccw90" | "270" | "rotate-ccw" => 8,
        other => bail!("未知方向 `{other}`，可用：normal/cw90/ccw90/180/mirror-h/mirror-v 或 1-8"),
    };
    Ok(ExifTag::Orientation(vec![code]))
}

/// 从 metadata 删除某个命名标签，返回是否删除了内容。
pub fn remove_named(metadata: &mut Metadata, name: &str) -> Result<bool> {
    let template = tag_template(name).with_context(|| format!("不支持删除的标签名 `{name}`"))?;
    let removed = metadata.remove_tag_by_hex_group(template.as_u16(), template.get_group());
    Ok(removed > 0)
}

fn canonical(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "")
}

// ============================ GPS ============================

/// 一次 GPS 定位。
#[derive(Debug, Clone, Copy)]
pub struct GpsFix {
    pub lat: f64,
    pub lon: f64,
    pub alt: Option<f64>,
}

/// 构造一组 GPS 标签（经纬度以十进制度输入）。
pub fn gps_tags(lat: f64, lon: f64, alt: Option<f64>) -> Vec<ExifTag> {
    let (lat_ref, lat_abs) = if lat < 0.0 { ("S", -lat) } else { ("N", lat) };
    let (lon_ref, lon_abs) = if lon < 0.0 { ("W", -lon) } else { ("E", lon) };
    let mut tags = vec![
        ExifTag::GPSVersionID(vec![2, 3, 0, 0]),
        ExifTag::GPSLatitudeRef(lat_ref.to_string()),
        ExifTag::GPSLatitude(to_dms(lat_abs)),
        ExifTag::GPSLongitudeRef(lon_ref.to_string()),
        ExifTag::GPSLongitude(to_dms(lon_abs)),
    ];
    if let Some(a) = alt {
        tags.push(ExifTag::GPSAltitudeRef(vec![if a < 0.0 { 1 } else { 0 }]));
        tags.push(ExifTag::GPSAltitude(vec![uR64::from(a.abs())]));
    }
    tags
}

/// 读取当前 GPS 定位（若存在）。
pub fn read_gps(metadata: &Metadata) -> Option<GpsFix> {
    let lat_dms = gps_rationals(metadata, &ExifTag::GPSLatitude(Vec::new()))?;
    let lon_dms = gps_rationals(metadata, &ExifTag::GPSLongitude(Vec::new()))?;
    if lat_dms.len() < 3 || lon_dms.len() < 3 {
        return None;
    }
    let mut lat = dms_to_decimal(&lat_dms);
    let mut lon = dms_to_decimal(&lon_dms);
    if let Some(r) = metadata
        .get_tag(&ExifTag::GPSLatitudeRef(String::new()))
        .next()
        && string_value(r).starts_with('S')
    {
        lat = -lat;
    }
    if let Some(r) = metadata
        .get_tag(&ExifTag::GPSLongitudeRef(String::new()))
        .next()
        && string_value(r).starts_with('W')
    {
        lon = -lon;
    }
    let alt = gps_rationals(metadata, &ExifTag::GPSAltitude(Vec::new()))
        .and_then(|v| v.first().copied())
        .map(|a| {
            let below = metadata
                .get_tag(&ExifTag::GPSAltitudeRef(Vec::new()))
                .next()
                .map(|t| t.value_as_u8_vec(&metadata.get_endian()).first().copied() == Some(1))
                .unwrap_or(false);
            if below { -a } else { a }
        });
    Some(GpsFix { lat, lon, alt })
}

/// 删除所有 GPS 标签。
pub fn remove_gps(metadata: &mut Metadata) -> usize {
    let mut removed = 0;
    for hex in 0x0000u16..=0x001f {
        removed += metadata.remove_tag_by_hex_group(hex, ExifTagGroup::GPS);
    }
    removed += metadata.remove_tag_by_hex_group(0x8825, ExifTagGroup::GENERIC);
    removed
}

fn to_dms(deg: f64) -> Vec<uR64> {
    // 以「百分之一弧秒」为单位分解，避免 0.1×60 的浮点误差产生 5'60" 这类不规范值
    let total = (deg.abs() * 360_000.0).round() as u64;
    let d = (total / 360_000) as u32;
    let m = ((total % 360_000) / 6_000) as u32;
    let cs = (total % 6_000) as u32; // 剩余的百分之一弧秒
    vec![
        uR64::from(d),
        uR64::from(m),
        uR64 {
            nominator: cs,
            denominator: 100,
        },
    ]
}

fn dms_to_decimal(v: &[f64]) -> f64 {
    v.first().copied().unwrap_or(0.0)
        + v.get(1).copied().unwrap_or(0.0) / 60.0
        + v.get(2).copied().unwrap_or(0.0) / 3600.0
}

fn gps_rationals(metadata: &Metadata, probe: &ExifTag) -> Option<Vec<f64>> {
    metadata.get_tag(probe).next().and_then(|t| match t {
        ExifTag::GPSLatitude(v) | ExifTag::GPSLongitude(v) | ExifTag::GPSAltitude(v) => {
            Some(v.iter().map(|r| f64::from(r.clone())).collect())
        }
        _ => None,
    })
}

// ============================ 方向（rotate）============================

/// 无损旋转/镜像操作。
#[derive(Debug, Clone, Copy)]
pub enum RotateOp {
    Cw,
    Ccw,
    Rot180,
    FlipH,
    FlipV,
    Reset,
}

/// 读取当前方向码（1-8），缺省为 1（正常）。
pub fn read_orientation(metadata: &Metadata) -> u16 {
    metadata
        .get_tag(&ExifTag::Orientation(Vec::new()))
        .next()
        .and_then(|t| match t {
            ExifTag::Orientation(v) => v.first().copied(),
            _ => None,
        })
        .filter(|c| (1..=8).contains(c))
        .unwrap_or(1)
}

/// 在当前方向基础上叠加一个操作，返回新的方向码。
///
/// EXIF 方向 = 把存储图像变换到正确显示的操作，构成正方形二面体群 D4。
/// 这里用 2×2 变换矩阵做群运算：新变换 = 操作矩阵 × 旧变换矩阵。
pub fn compose_orientation(current: u16, op: RotateOp) -> u16 {
    if let RotateOp::Reset = op {
        return 1;
    }
    let result = mat_mul(op_matrix(op), orient_matrix(current));
    matrix_to_orient(result)
}

type Mat = [[i32; 2]; 2];

fn mat_mul(a: Mat, b: Mat) -> Mat {
    [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0],
            a[0][0] * b[0][1] + a[0][1] * b[1][1],
        ],
        [
            a[1][0] * b[0][0] + a[1][1] * b[1][0],
            a[1][0] * b[0][1] + a[1][1] * b[1][1],
        ],
    ]
}

/// 方向码 → 变换矩阵（存储→显示，屏幕坐标 x 向右、y 向下）。
fn orient_matrix(o: u16) -> Mat {
    match o {
        1 => [[1, 0], [0, 1]],   // 正常
        2 => [[-1, 0], [0, 1]],  // 水平镜像
        3 => [[-1, 0], [0, -1]], // 旋转 180
        4 => [[1, 0], [0, -1]],  // 垂直镜像
        5 => [[0, 1], [1, 0]],   // 转置（主对角镜像）
        6 => [[0, -1], [1, 0]],  // 顺时针 90
        7 => [[0, -1], [-1, 0]], // 反对角镜像
        8 => [[0, 1], [-1, 0]],  // 逆时针 90
        _ => [[1, 0], [0, 1]],
    }
}

fn matrix_to_orient(m: Mat) -> u16 {
    (1..=8).find(|&o| orient_matrix(o) == m).unwrap_or(1)
}

fn op_matrix(op: RotateOp) -> Mat {
    match op {
        RotateOp::Cw => [[0, -1], [1, 0]],
        RotateOp::Ccw => [[0, 1], [-1, 0]],
        RotateOp::Rot180 => [[-1, 0], [0, -1]],
        RotateOp::FlipH => [[-1, 0], [0, 1]],
        RotateOp::FlipV => [[1, 0], [0, -1]],
        RotateOp::Reset => [[1, 0], [0, 1]],
    }
}

/// 方向码的中文说明。
pub fn orientation_desc(code: u16) -> &'static str {
    match code {
        1 => "正常",
        2 => "水平镜像",
        3 => "旋转180°",
        4 => "垂直镜像",
        5 => "转置",
        6 => "顺时针90°",
        7 => "反对角镜像",
        8 => "逆时针90°",
        _ => "未知",
    }
}

// ============================ 复制（copy）============================

/// 从 `from` 复制符合条件的标签到 `to`，返回复制的标签数。
pub fn copy_tags(from: &Metadata, to: &mut Metadata, all: bool, time: bool, gps: bool) -> usize {
    let mut count = 0;
    for ifd in from.get_ifds() {
        for tag in ifd.get_tags() {
            let take = if all {
                copyable_in_all(tag)
            } else {
                (time && is_time_tag(tag.as_u16())) || (gps && tag.get_group() == ExifTagGroup::GPS)
            };
            if take {
                to.set_tag(tag.clone());
                count += 1;
            }
        }
    }
    count
}

fn is_time_tag(hex: u16) -> bool {
    matches!(
        hex,
        0x0132 | 0x9003 | 0x9004 | 0x9010 | 0x9011 | 0x9012 | 0x9290 | 0x9291 | 0x9292
    )
}

/// `--all` 模式下是否复制该标签：排除偏移指针、缩略图、以及与具体图像绑定的结构字段。
fn copyable_in_all(tag: &ExifTag) -> bool {
    // 偏移/缩略图等由编码器管理，不复制
    if matches!(
        tag,
        ExifTag::ExifOffset(_)
            | ExifTag::GPSInfo(_)
            | ExifTag::InteropOffset(_)
            | ExifTag::StripOffsets(_, _)
            | ExifTag::StripByteCounts(_)
            | ExifTag::ThumbnailOffset(_, _)
            | ExifTag::ThumbnailLength(_)
    ) {
        return false;
    }
    // 与像素/尺寸/方向绑定的结构字段，复制到别的图像会出错
    !matches!(
        tag.as_u16(),
        0x0100 // ImageWidth
            | 0x0101 // ImageHeight
            | 0x0102 // BitsPerSample
            | 0x0103 // Compression
            | 0x0106 // PhotometricInterpretation
            | 0x0112 // Orientation
            | 0x0115 // SamplesPerPixel
            | 0x0116 // RowsPerStrip
            | 0x011c // PlanarConfiguration
            | 0xa002 // ExifImageWidth
            | 0xa003 // ExifImageHeight
    )
}

// ============================ 显示（show）============================

/// 一条可读的标签记录。
pub struct TagView {
    pub group: String,
    pub name: String,
    pub hex: u16,
    pub value: String,
}

/// 把 metadata 展平成一组可读记录，按 IFD 顺序。
pub fn list_tags(metadata: &Metadata) -> Vec<TagView> {
    let endian = metadata.get_endian();
    let mut out = Vec::new();
    for ifd in metadata.get_ifds() {
        let group = group_name(ifd.get_ifd_type()).to_string();
        for tag in ifd.get_tags() {
            // 跳过内部使用的偏移量指针，对用户无意义
            if matches!(
                tag,
                ExifTag::ExifOffset(_) | ExifTag::GPSInfo(_) | ExifTag::InteropOffset(_)
            ) {
                continue;
            }
            out.push(TagView {
                group: group.clone(),
                name: tag_name(tag),
                hex: tag.as_u16(),
                value: format_value(tag, &endian),
            });
        }
    }
    out
}

pub fn group_name(g: ExifTagGroup) -> &'static str {
    match g {
        ExifTagGroup::GENERIC => "IFD0",
        ExifTagGroup::EXIF => "EXIF",
        ExifTagGroup::INTEROP => "Interop",
        ExifTagGroup::GPS => "GPS",
    }
}

/// 标签名称（可读）。未知标签以十六进制表示。
pub fn tag_name(tag: &ExifTag) -> String {
    if tag.is_unknown() {
        return format!("Unknown_0x{:04X}", tag.as_u16());
    }
    let dbg = format!("{tag:?}");
    match dbg.split_once('(') {
        Some((name, _)) => name.to_string(),
        None => dbg,
    }
}

/// 取字符串型标签的值（去掉 NUL 与空白）。
fn string_value(tag: &ExifTag) -> String {
    let bytes = tag.value_as_u8_vec(&Endian::Little);
    String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .trim()
        .to_string()
}

/// 把任意标签的值格式化为可读字符串。
pub fn format_value(tag: &ExifTag, endian: &Endian) -> String {
    let fmt = tag.format();
    if matches!(fmt, ExifTagFormat::STRING) {
        return string_value(tag);
    }

    let bytes = tag.value_as_u8_vec(endian);
    let little = matches!(endian, Endian::Little);
    let bpc = fmt.bytes_per_component() as usize;
    let n = bytes.len().checked_div(bpc).unwrap_or(0);

    let items: Vec<String> = match fmt {
        ExifTagFormat::INT8U => bytes.iter().map(|b| b.to_string()).collect(),
        ExifTagFormat::INT8S => bytes.iter().map(|b| (*b as i8).to_string()).collect(),
        ExifTagFormat::INT16U => (0..n)
            .map(|i| read_u16(&bytes[i * 2..], little).to_string())
            .collect(),
        ExifTagFormat::INT16S => (0..n)
            .map(|i| read_i16(&bytes[i * 2..], little).to_string())
            .collect(),
        ExifTagFormat::INT32U => (0..n)
            .map(|i| read_u32(&bytes[i * 4..], little).to_string())
            .collect(),
        ExifTagFormat::INT32S => (0..n)
            .map(|i| read_i32(&bytes[i * 4..], little).to_string())
            .collect(),
        ExifTagFormat::FLOAT => (0..n)
            .map(|i| read_f32(&bytes[i * 4..], little).to_string())
            .collect(),
        ExifTagFormat::DOUBLE => (0..n)
            .map(|i| read_f64(&bytes[i * 8..], little).to_string())
            .collect(),
        ExifTagFormat::RATIONAL64U => (0..n)
            .map(|i| {
                let num = read_u32(&bytes[i * 8..], little);
                let den = read_u32(&bytes[i * 8 + 4..], little);
                fmt_ratio(num as f64, den as f64)
            })
            .collect(),
        ExifTagFormat::RATIONAL64S => (0..n)
            .map(|i| {
                let num = read_i32(&bytes[i * 8..], little);
                let den = read_i32(&bytes[i * 8 + 4..], little);
                fmt_ratio(num as f64, den as f64)
            })
            .collect(),
        ExifTagFormat::UNDEF => return format_undef(&bytes),
        ExifTagFormat::STRING => unreachable!(),
    };
    items.join(", ")
}

fn fmt_ratio(num: f64, den: f64) -> String {
    if den == 0.0 {
        return "0".to_string();
    }
    if num % den == 0.0 {
        format!("{}", (num / den) as i64)
    } else if den == 1.0 {
        format!("{num}")
    } else {
        // 保留分数形式，同时给出近似小数
        format!("{}/{} ({:.4})", num as i64, den as i64, num / den)
    }
}

fn format_undef(bytes: &[u8]) -> String {
    // 去掉常见的字符集前缀（如 UserComment 的 "ASCII\0\0\0"）
    let body = if bytes.len() > 8 && &bytes[..5] == b"ASCII" {
        &bytes[8..]
    } else {
        bytes
    };
    let printable = body
        .iter()
        .all(|&b| b == 0 || (0x20..=0x7e).contains(&b) || b >= 0x80);
    if printable && !body.is_empty() {
        String::from_utf8_lossy(body)
            .trim_end_matches('\0')
            .trim()
            .to_string()
    } else {
        format!("<{} 字节>", bytes.len())
    }
}

fn read_u16(b: &[u8], little: bool) -> u16 {
    if little {
        u16::from_le_bytes([b[0], b[1]])
    } else {
        u16::from_be_bytes([b[0], b[1]])
    }
}
fn read_i16(b: &[u8], little: bool) -> i16 {
    if little {
        i16::from_le_bytes([b[0], b[1]])
    } else {
        i16::from_be_bytes([b[0], b[1]])
    }
}
fn read_u32(b: &[u8], little: bool) -> u32 {
    let a = [b[0], b[1], b[2], b[3]];
    if little {
        u32::from_le_bytes(a)
    } else {
        u32::from_be_bytes(a)
    }
}
fn read_i32(b: &[u8], little: bool) -> i32 {
    let a = [b[0], b[1], b[2], b[3]];
    if little {
        i32::from_le_bytes(a)
    } else {
        i32::from_be_bytes(a)
    }
}
fn read_f32(b: &[u8], little: bool) -> f32 {
    let a = [b[0], b[1], b[2], b[3]];
    if little {
        f32::from_le_bytes(a)
    } else {
        f32::from_be_bytes(a)
    }
}
fn read_f64(b: &[u8], little: bool) -> f64 {
    let a = [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]];
    if little {
        f64::from_le_bytes(a)
    } else {
        f64::from_be_bytes(a)
    }
}

// ============================ 落盘内部实现 ============================

fn prepare_backup(path: &Path, opts: &WriteOpts) -> Result<()> {
    if opts.backup {
        let bak = backup_path(path);
        if !bak.exists() {
            fs::copy(path, &bak).with_context(|| format!("备份失败：{}", bak.display()))?;
        }
    }
    Ok(())
}

fn snapshot_times(path: &Path, preserve: bool) -> Option<(filetime::FileTime, filetime::FileTime)> {
    if !preserve {
        return None;
    }
    fs::metadata(path).ok().map(|m| {
        (
            filetime::FileTime::from_last_access_time(&m),
            filetime::FileTime::from_last_modification_time(&m),
        )
    })
}

fn restore_times(path: &Path, times: Option<(filetime::FileTime, filetime::FileTime)>) {
    if let Some((atime, mtime)) = times {
        let _ = filetime::set_file_times(path, atime, mtime);
    }
}

/// 某文件对应的 .bak 备份路径（`<文件名>.bak`）。
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// 原子替换：同目录写临时文件 → fsync → 重命名覆盖。
fn atomic_replace(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    let tmp = match dir {
        Some(d) => d.join(format!(".{file_name}.pkick.tmp")),
        None => PathBuf::from(format!(".{file_name}.pkick.tmp")),
    };

    {
        let mut f =
            File::create(&tmp).with_context(|| format!("创建临时文件失败：{}", tmp.display()))?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        bail!("重命名覆盖失败：{e}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_cw_from_normal() {
        assert_eq!(compose_orientation(1, RotateOp::Cw), 6);
        assert_eq!(compose_orientation(1, RotateOp::Ccw), 8);
        assert_eq!(compose_orientation(1, RotateOp::Rot180), 3);
        assert_eq!(compose_orientation(1, RotateOp::FlipH), 2);
        assert_eq!(compose_orientation(1, RotateOp::FlipV), 4);
    }

    #[test]
    fn rotate_cw_full_circle() {
        // 顺时针转四次回到原点：1 -> 6 -> 3 -> 8 -> 1
        let mut o = 1u16;
        for expected in [6, 3, 8, 1] {
            o = compose_orientation(o, RotateOp::Cw);
            assert_eq!(o, expected);
        }
    }

    #[test]
    fn rotate_cw_then_ccw_is_identity() {
        for start in 1..=8u16 {
            let there = compose_orientation(start, RotateOp::Cw);
            let back = compose_orientation(there, RotateOp::Ccw);
            assert_eq!(back, start, "起点 {start} 往返未复原");
        }
    }

    #[test]
    fn rotate_compose_with_existing() {
        // 已镜像的图再顺时针 90 = 方向 7
        assert_eq!(compose_orientation(2, RotateOp::Cw), 7);
        // 顺时针两次 = 180
        assert_eq!(compose_orientation(6, RotateOp::Cw), 3);
    }

    #[test]
    fn rotate_180_twice_is_identity() {
        for start in 1..=8u16 {
            let twice = compose_orientation(
                compose_orientation(start, RotateOp::Rot180),
                RotateOp::Rot180,
            );
            assert_eq!(twice, start);
        }
    }

    #[test]
    fn flip_twice_is_identity() {
        for start in 1..=8u16 {
            let h =
                compose_orientation(compose_orientation(start, RotateOp::FlipH), RotateOp::FlipH);
            assert_eq!(h, start);
            let v =
                compose_orientation(compose_orientation(start, RotateOp::FlipV), RotateOp::FlipV);
            assert_eq!(v, start);
        }
    }

    #[test]
    fn reset_always_normal() {
        for start in 1..=8u16 {
            assert_eq!(compose_orientation(start, RotateOp::Reset), 1);
        }
    }

    #[test]
    fn every_orientation_matrix_is_unique() {
        // 保证矩阵↔方向码是双射，compose 才可靠
        for a in 1..=8u16 {
            for b in 1..=8u16 {
                if a != b {
                    assert_ne!(orient_matrix(a), orient_matrix(b));
                }
            }
        }
    }
}

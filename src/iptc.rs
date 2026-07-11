//! 旧版 IPTC-IIM 元数据读写（JPEG 的 APP13 / Photoshop 8BIM 图像资源块）。
//!
//! 结构层次：JPEG APP13 段 → "Photoshop 3.0\0" 签名 → 若干 8BIM 资源块 →
//! 其中资源 ID 0x0404 的块内是 IPTC-IIM 数据集流。
//!
//! 写入时保留 0x0404 以外的其它 8BIM 块（缩略图、色彩配置等），只替换 IPTC 块；
//! 字符集统一按 UTF-8 处理并写入 1:90 CodedCharacterSet 标记（ESC % G）。

use anyhow::{bail, Result};

const PS_SIG: &[u8] = b"Photoshop 3.0\0"; // 14 字节
const IPTC_ID: u16 = 0x0404;
/// CodedCharacterSet 的 UTF-8 标记：ESC % G
const CHARSET_UTF8: &[u8] = &[0x1B, 0x25, 0x47];

// ============================ 数据结构 ============================

/// 一个 IIM 数据集。
#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    pub record: u8,
    pub number: u8,
    pub data: Vec<u8>,
}

/// 一个 8BIM 图像资源块。
#[derive(Debug, Clone)]
struct Irb {
    id: u16,
    name: Vec<u8>,
    data: Vec<u8>,
}

/// 一批 IPTC 编辑操作。
#[derive(Debug, Default)]
pub struct IptcEdit {
    /// (record, dataset, 值列表)。值列表为多项时写成可重复数据集。
    pub sets: Vec<(u8, u8, Vec<String>)>,
    /// 要删除的 (record, dataset)
    pub removes: Vec<(u8, u8)>,
}

impl IptcEdit {
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty() && self.removes.is_empty()
    }
}

// ============================ 字段名映射 ============================

/// record 2（应用记录）常见数据集号 → 可读名。
pub fn field_name(record: u8, number: u8) -> String {
    if record == 2 {
        let n = match number {
            5 => "Title",
            15 => "Category",
            20 => "SupplementalCategory",
            25 => "Keywords",
            40 => "Instructions",
            55 => "DateCreated",
            60 => "TimeCreated",
            80 => "Creator",
            85 => "CreatorTitle",
            90 => "City",
            92 => "Sublocation",
            95 => "Province/State",
            101 => "Country",
            103 => "TransmissionRef",
            105 => "Headline",
            110 => "Credit",
            115 => "Source",
            116 => "Copyright",
            120 => "Caption",
            122 => "CaptionWriter",
            _ => return format!("IPTC:2:{number}"),
        };
        n.to_string()
    } else {
        format!("IPTC:{record}:{number}")
    }
}

/// CLI 字段名 → (record, dataset)。
pub fn resolve_field(name: &str) -> Option<(u8, u8)> {
    let canon = name.trim().to_ascii_lowercase().replace(['-', '_', ' '], "");
    // 也接受 "2:105" 形式
    if let Some((r, d)) = canon.split_once(':') {
        if let (Ok(r), Ok(d)) = (r.parse::<u8>(), d.parse::<u8>()) {
            return Some((r, d));
        }
    }
    Some(match canon.as_str() {
        "title" | "objectname" => (2, 5),
        "category" => (2, 15),
        "keywords" | "keyword" => (2, 25),
        "instructions" => (2, 40),
        "datecreated" => (2, 55),
        "creator" | "byline" | "author" => (2, 80),
        "city" => (2, 90),
        "sublocation" => (2, 92),
        "state" | "province" => (2, 95),
        "country" => (2, 101),
        "headline" => (2, 105),
        "credit" => (2, 110),
        "source" => (2, 115),
        "copyright" => (2, 116),
        "caption" | "description" => (2, 120),
        "captionwriter" => (2, 122),
        _ => return None,
    })
}

// ============================ 应用编辑 ============================

/// 把编辑应用到现有数据集上（保留未涉及的数据集），返回新数据集列表。
pub fn apply(existing: &[Dataset], edit: &IptcEdit) -> Vec<Dataset> {
    let mut result: Vec<Dataset> = existing.to_vec();

    // 删除：被 set 覆盖的和显式 remove 的
    let overwritten: Vec<(u8, u8)> = edit.sets.iter().map(|(r, n, _)| (*r, *n)).collect();
    result.retain(|d| {
        !overwritten.contains(&(d.record, d.number))
            && !edit.removes.contains(&(d.record, d.number))
    });

    // 追加 set 的值（可重复字段写成多条）
    for (r, n, values) in &edit.sets {
        for v in values {
            result.push(Dataset {
                record: *r,
                number: *n,
                data: v.as_bytes().to_vec(),
            });
        }
    }

    // 确保 UTF-8 字符集标记存在（写入的值均为 UTF-8）
    let has_charset = result.iter().any(|d| d.record == 1 && d.number == 90);
    let has_content = result.iter().any(|d| d.record == 2);
    if !has_charset && has_content {
        result.insert(
            0,
            Dataset {
                record: 1,
                number: 90,
                data: CHARSET_UTF8.to_vec(),
            },
        );
    }

    result
}

// ============================ IIM 编解码 ============================

fn parse_iim(d: &[u8]) -> Vec<Dataset> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 5 <= d.len() {
        if d[i] != 0x1C {
            break;
        }
        let record = d[i + 1];
        let number = d[i + 2];
        let len = ((d[i + 3] as usize) << 8) | d[i + 4] as usize;
        i += 5;
        if len >= 0x8000 {
            break; // 扩展长度暂不支持
        }
        if i + len > d.len() {
            break;
        }
        out.push(Dataset {
            record,
            number,
            data: d[i..i + len].to_vec(),
        });
        i += len;
    }
    out
}

fn serialize_iim(datasets: &[Dataset]) -> Vec<u8> {
    let mut sorted = datasets.to_vec();
    // 稳定排序：按 record、number 升序，同号内保持插入顺序（可重复字段）
    sorted.sort_by(|a, b| (a.record, a.number).cmp(&(b.record, b.number)));

    let mut out = Vec::new();
    for ds in &sorted {
        let len = ds.data.len().min(0x7FFF);
        out.push(0x1C);
        out.push(ds.record);
        out.push(ds.number);
        out.push((len >> 8) as u8);
        out.push((len & 0xFF) as u8);
        out.extend_from_slice(&ds.data[..len]);
    }
    out
}

// ============================ 8BIM 编解码 ============================

fn parse_irbs(d: &[u8]) -> Vec<Irb> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= d.len() {
        if &d[i..i + 4] != b"8BIM" {
            break;
        }
        i += 4;
        if i + 2 > d.len() {
            break;
        }
        let id = ((d[i] as u16) << 8) | d[i + 1] as u16;
        i += 2;
        // Pascal 名（1 字节长度 + 名字，整体补齐到偶数）
        if i >= d.len() {
            break;
        }
        let name_len = d[i] as usize;
        if i + 1 + name_len > d.len() {
            break;
        }
        let name = d[i + 1..i + 1 + name_len].to_vec();
        let name_field = 1 + name_len;
        i += name_field + (name_field & 1);
        if i + 4 > d.len() {
            break;
        }
        let size = ((d[i] as usize) << 24)
            | ((d[i + 1] as usize) << 16)
            | ((d[i + 2] as usize) << 8)
            | d[i + 3] as usize;
        i += 4;
        if i + size > d.len() {
            break;
        }
        let data = d[i..i + size].to_vec();
        i += size + (size & 1); // 数据补齐到偶数
        out.push(Irb { id, name, data });
    }
    out
}

fn serialize_irbs(irbs: &[Irb]) -> Vec<u8> {
    let mut out = Vec::new();
    for irb in irbs {
        out.extend_from_slice(b"8BIM");
        out.extend_from_slice(&irb.id.to_be_bytes());
        let name_len = irb.name.len().min(255);
        out.push(name_len as u8);
        out.extend_from_slice(&irb.name[..name_len]);
        if (1 + name_len) & 1 == 1 {
            out.push(0); // 名字补齐到偶数
        }
        out.extend_from_slice(&(irb.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&irb.data);
        if irb.data.len() & 1 == 1 {
            out.push(0); // 数据补齐到偶数
        }
    }
    out
}

// ============================ JPEG APP13 段手术 ============================

/// 查找带 Photoshop 签名的 APP13 段，返回 (段起始, 段结束(不含), 签名后的 IRB 载荷)。
fn find_app13(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF {
            break;
        }
        let marker = bytes[i + 1];
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if marker == 0xDA {
            break;
        }
        let len = ((bytes[i + 2] as usize) << 8) | bytes[i + 3] as usize;
        if len < 2 || i + 2 + len > bytes.len() {
            break;
        }
        let payload = &bytes[i + 4..i + 2 + len];
        if marker == 0xED && payload.starts_with(PS_SIG) {
            let irb = payload[PS_SIG.len()..].to_vec();
            return Some((i, i + 2 + len, irb));
        }
        i += 2 + len;
    }
    None
}

/// 读取所有 IPTC 数据集。
pub fn read_datasets(bytes: &[u8]) -> Vec<Dataset> {
    let Some((_, _, irb_payload)) = find_app13(bytes) else {
        return Vec::new();
    };
    for irb in parse_irbs(&irb_payload) {
        if irb.id == IPTC_ID {
            return parse_iim(&irb.data);
        }
    }
    Vec::new()
}

/// 为 show 读出 (字段名, 值) 列表；可重复字段合并显示。
pub fn read_properties(bytes: &[u8]) -> Vec<(String, String)> {
    let datasets = read_datasets(bytes);
    let utf8 = datasets
        .iter()
        .any(|d| d.record == 1 && d.number == 90 && d.data == CHARSET_UTF8);

    let mut out: Vec<(String, String)> = Vec::new();
    for ds in &datasets {
        if ds.record == 1 {
            continue; // 跳过封套记录（字符集等内部信息）
        }
        let name = field_name(ds.record, ds.number);
        let value = decode_text(&ds.data, utf8);
        // 合并可重复字段
        if let Some(entry) = out.iter_mut().find(|(n, _)| *n == name) {
            entry.1.push_str("; ");
            entry.1.push_str(&value);
        } else {
            out.push((name, value));
        }
    }
    out
}

fn decode_text(data: &[u8], utf8: bool) -> String {
    if utf8 {
        String::from_utf8_lossy(data).into_owned()
    } else {
        match std::str::from_utf8(data) {
            Ok(s) => s.to_string(),
            // 回退 Latin-1
            Err(_) => data.iter().map(|&b| b as char).collect(),
        }
    }
}

/// 用给定数据集重建并写入 JPEG 的 IPTC（保留其它 8BIM 块）。
pub fn set_jpeg_iptc(bytes: &mut Vec<u8>, datasets: &[Dataset]) -> Result<()> {
    let iim = serialize_iim(datasets);

    let mut irbs = match find_app13(bytes) {
        Some((_, _, payload)) => parse_irbs(&payload),
        None => Vec::new(),
    };
    irbs.retain(|irb| irb.id != IPTC_ID);
    irbs.insert(
        0,
        Irb {
            id: IPTC_ID,
            name: Vec::new(),
            data: iim,
        },
    );

    write_app13(bytes, &irbs)
}

/// 删除 IPTC；若 APP13 中不再有其它块则移除整个段。返回是否删除了内容。
pub fn remove_jpeg_iptc(bytes: &mut Vec<u8>) -> bool {
    let Some((start, end, payload)) = find_app13(bytes) else {
        return false;
    };
    let mut irbs = parse_irbs(&payload);
    if !irbs.iter().any(|irb| irb.id == IPTC_ID) {
        return false;
    }
    irbs.retain(|irb| irb.id != IPTC_ID);
    if irbs.is_empty() {
        bytes.drain(start..end);
    } else if write_app13(bytes, &irbs).is_err() {
        return false;
    }
    true
}

/// 用给定的 8BIM 块集合重写（替换或插入）APP13 段。
fn write_app13(bytes: &mut Vec<u8>, irbs: &[Irb]) -> Result<()> {
    let mut payload = PS_SIG.to_vec();
    payload.extend_from_slice(&serialize_irbs(irbs));

    let seg_len = payload.len() + 2;
    if seg_len > 0xFFFF {
        bail!("IPTC/APP13 数据过大（{} 字节），暂不支持分段写入", payload.len());
    }
    let mut segment = Vec::with_capacity(payload.len() + 4);
    segment.push(0xFF);
    segment.push(0xED);
    segment.push((seg_len >> 8) as u8);
    segment.push((seg_len & 0xFF) as u8);
    segment.extend_from_slice(&payload);

    if let Some((start, end, _)) = find_app13(bytes) {
        bytes.splice(start..end, segment);
    } else {
        // 插到所有前导 APPn 段之后，保证 EXIF 段仍是第一个 APP1
        let at = crate::xmp::jpeg_app_insert_pos(bytes);
        bytes.splice(at..at, segment);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds(record: u8, number: u8, s: &str) -> Dataset {
        Dataset { record, number, data: s.as_bytes().to_vec() }
    }

    #[test]
    fn iim_roundtrip() {
        let input = vec![ds(2, 5, "Title"), ds(2, 25, "kw1"), ds(2, 25, "kw2")];
        let bytes = serialize_iim(&input);
        let parsed = parse_iim(&bytes);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], ds(2, 5, "Title"));
        assert_eq!(parsed[1], ds(2, 25, "kw1"));
        assert_eq!(parsed[2], ds(2, 25, "kw2"));
    }

    #[test]
    fn irb_roundtrip_preserves_others() {
        let irbs = vec![
            Irb { id: 0x0404, name: vec![], data: vec![1, 2, 3] },      // IPTC（奇数长度→需补齐）
            Irb { id: 0x040C, name: vec![], data: vec![9, 9, 9, 9] },   // 缩略图（占位）
        ];
        let bytes = serialize_irbs(&irbs);
        let parsed = parse_irbs(&bytes);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, 0x0404);
        assert_eq!(parsed[0].data, vec![1, 2, 3]);
        assert_eq!(parsed[1].id, 0x040C);
        assert_eq!(parsed[1].data, vec![9, 9, 9, 9]);
    }

    #[test]
    fn apply_sets_removes_and_adds_charset() {
        let existing = vec![ds(2, 5, "Old"), ds(2, 116, "Copyright")];
        let edit = IptcEdit {
            sets: vec![(2, 5, vec!["New".into()]), (2, 25, vec!["a".into(), "b".into()])],
            removes: vec![(2, 116)],
        };
        let out = apply(&existing, &edit);
        // 旧标题被替换
        assert!(out.iter().any(|d| d.record == 2 && d.number == 5 && d.data == b"New"));
        assert!(!out.iter().any(|d| d.data == b"Old"));
        // 版权被删
        assert!(!out.iter().any(|d| d.record == 2 && d.number == 116));
        // 关键词两条
        assert_eq!(out.iter().filter(|d| d.record == 2 && d.number == 25).count(), 2);
        // UTF-8 字符集标记已加
        assert!(out.iter().any(|d| d.record == 1 && d.number == 90 && d.data == CHARSET_UTF8));
    }

    #[test]
    fn app13_segment_roundtrip() {
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xD9];
        assert!(read_datasets(&jpeg).is_empty());
        let datasets = apply(&[], &IptcEdit {
            sets: vec![(2, 5, vec!["测试标题".into()]), (2, 25, vec!["关键词".into()])],
            removes: vec![],
        });
        set_jpeg_iptc(&mut jpeg, &datasets).unwrap();
        let props = read_properties(&jpeg);
        assert!(props.iter().any(|(k, v)| k == "Title" && v == "测试标题"));
        assert!(props.iter().any(|(k, v)| k == "Keywords" && v == "关键词"));
        assert!(remove_jpeg_iptc(&mut jpeg));
        assert!(read_datasets(&jpeg).is_empty());
    }

    #[test]
    fn set_preserves_other_irb() {
        // 构造一个含缩略图 IRB 的 APP13，再写 IPTC，缩略图应保留
        let mut jpeg = vec![0xFF, 0xD8];
        let irbs = vec![Irb { id: 0x040C, name: vec![], data: vec![7; 10] }];
        let mut payload = PS_SIG.to_vec();
        payload.extend_from_slice(&serialize_irbs(&irbs));
        let seg_len = payload.len() + 2;
        jpeg.extend_from_slice(&[0xFF, 0xED, (seg_len >> 8) as u8, (seg_len & 0xFF) as u8]);
        jpeg.extend_from_slice(&payload);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);

        let datasets = apply(&[], &IptcEdit { sets: vec![(2, 5, vec!["T".into()])], removes: vec![] });
        set_jpeg_iptc(&mut jpeg, &datasets).unwrap();

        let (_, _, payload) = find_app13(&jpeg).unwrap();
        let irbs = parse_irbs(&payload);
        assert!(irbs.iter().any(|i| i.id == 0x040C && i.data == vec![7; 10]));
        assert!(irbs.iter().any(|i| i.id == IPTC_ID));
    }
}

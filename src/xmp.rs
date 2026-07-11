//! XMP（Adobe RDF/XML 元数据）读写，聚焦 JPEG。
//!
//! XMP 以独立的 APP1 段（Adobe 签名）嵌在 JPEG 里，与 EXIF 段互不影响，
//! 因此本模块自行做 JPEG 段手术；RDF/XML 用 quick-xml 解析。
//!
//! 写入采用「事件透传」策略：逐 XML 事件复制原包，只拦截被管理的属性，
//! 从而**保留所有未知的 XMP 属性和结构**，只增改用户指定的那几个。

use std::io::Write as _;
use std::path::Path;

use anyhow::{Result, bail};
use quick_xml::escape::escape;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

/// JPEG 中 XMP APP1 段的签名。
const XMP_SIG: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

// ============================ 属性值类型 ============================

/// XMP 属性值的形态。
#[derive(Debug, Clone)]
pub enum XmpValue {
    /// 简单文本，如 xmp:Rating。
    Simple(String),
    /// 语言可选文本（rdf:Alt），如 dc:title、dc:description。
    LangAlt(String),
    /// 有序数组（rdf:Seq），如 dc:creator。
    Seq(Vec<String>),
    /// 无序数组（rdf:Bag），如 dc:subject（关键词）。
    Bag(Vec<String>),
}

/// 一批 XMP 编辑操作。
#[derive(Debug, Default)]
pub struct XmpEdit {
    /// (限定名 如 "dc:title", 值)
    pub sets: Vec<(String, XmpValue)>,
    /// 要删除的限定名
    pub removes: Vec<String>,
}

impl XmpEdit {
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty() && self.removes.is_empty()
    }

    fn is_managed(&self, qname: &str) -> bool {
        self.sets.iter().any(|(q, _)| q == qname) || self.removes.iter().any(|q| q == qname)
    }

    fn fragment_for(&self, qname: &str) -> Option<String> {
        self.sets
            .iter()
            .find(|(q, _)| q == qname)
            .map(|(q, v)| build_fragment(q, v))
    }

    /// 本次编辑用到的命名空间前缀（用于确保 xmlns 声明存在）。
    fn used_prefixes(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self
            .sets
            .iter()
            .filter_map(|(q, _)| q.split(':').next())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// 已知命名空间前缀 → URI。
pub fn namespace_uri(prefix: &str) -> Option<&'static str> {
    Some(match prefix {
        "rdf" => RDF_NS,
        "dc" => "http://purl.org/dc/elements/1.1/",
        "xmp" => "http://ns.adobe.com/xap/1.0/",
        "photoshop" => "http://ns.adobe.com/photoshop/1.0/",
        "lr" => "http://ns.adobe.com/lightroom/1.0/",
        "xmpRights" => "http://ns.adobe.com/xap/1.0/rights/",
        "Iptc4xmpCore" => "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/",
        _ => return None,
    })
}

// ============================ 属性构建 ============================

fn build_fragment(qname: &str, value: &XmpValue) -> String {
    match value {
        XmpValue::Simple(v) => format!("<{qname}>{}</{qname}>", escape(v.as_str())),
        XmpValue::LangAlt(v) => format!(
            "<{qname}><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></{qname}>",
            escape(v.as_str())
        ),
        XmpValue::Seq(items) => wrap_array(qname, "Seq", items),
        XmpValue::Bag(items) => wrap_array(qname, "Bag", items),
    }
}

fn wrap_array(qname: &str, kind: &str, items: &[String]) -> String {
    let lis: String = items
        .iter()
        .map(|s| format!("<rdf:li>{}</rdf:li>", escape(s.as_str())))
        .collect();
    format!("<{qname}><rdf:{kind}>{lis}</rdf:{kind}></{qname}>")
}

// ============================ 应用编辑 ============================

/// 把编辑应用到（可选的）现有 XMP 包上，返回新包文本。
pub fn apply(existing: Option<&str>, edit: &XmpEdit) -> Result<String> {
    match existing {
        Some(pkt) if pkt.contains("rdf:Description") => edit_existing(pkt, edit),
        _ => Ok(build_fresh(edit)),
    }
}

/// 生成全新的 XMP 包。
fn build_fresh(edit: &XmpEdit) -> String {
    let mut xmlns = String::from(" xmlns:rdf=\"");
    xmlns.push_str(RDF_NS);
    xmlns.push('"');
    for prefix in edit.used_prefixes() {
        if prefix == "rdf" {
            continue;
        }
        if let Some(uri) = namespace_uri(prefix) {
            xmlns.push_str(&format!(" xmlns:{prefix}=\"{uri}\""));
        }
    }
    let body: String = edit
        .sets
        .iter()
        .map(|(q, v)| format!("\n   {}", build_fragment(q, v)))
        .collect();

    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n \
         <rdf:RDF xmlns:rdf=\"{RDF_NS}\">\n  \
         <rdf:Description rdf:about=\"\"{extra_ns}>{body}\n  </rdf:Description>\n \
         </rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>",
        extra_ns = strip_rdf(&xmlns),
        body = body,
    )
}

// build_fresh 里 rdf 已单独写，去掉重复的 rdf 声明
fn strip_rdf(xmlns: &str) -> String {
    xmlns.replacen(&format!(" xmlns:rdf=\"{RDF_NS}\""), "", 1)
}

/// 在现有包上做事件级别的增改删，保留未知内容。
fn edit_existing(packet: &str, edit: &XmpEdit) -> Result<String> {
    let mut reader = Reader::from_str(packet);
    reader.config_mut().check_end_names = false;
    let mut writer = Writer::new(Vec::new());

    let mut first_desc_pending = true;
    let mut in_desc = false;
    let mut child_depth: i32 = 0;
    let mut done: Vec<String> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = qname_string(e.name());
                if first_desc_pending && name == "rdf:Description" {
                    first_desc_pending = false;
                    in_desc = true;
                    child_depth = 0;
                    let start = rebuild_desc_start(&e, edit)?;
                    writer.write_event(Event::Start(start))?;
                } else if in_desc && child_depth == 0 && edit.is_managed(&name) {
                    // 管理的元素：整块跳过，若是 set 则写入新片段
                    let end = e.to_end().into_owned();
                    reader.read_to_end(end.name())?;
                    if let Some(frag) = edit.fragment_for(&name) {
                        writer.get_mut().write_all(frag.as_bytes())?;
                    }
                    remember(&mut done, name);
                } else {
                    if in_desc {
                        child_depth += 1;
                    }
                    writer.write_event(Event::Start(e))?;
                }
            }
            Ok(Event::Empty(e)) => {
                let name = qname_string(e.name());
                if first_desc_pending && name == "rdf:Description" {
                    // 自闭合 Description（仅属性形式）→ 展开为 Start + 子元素 + End
                    first_desc_pending = false;
                    let start = rebuild_desc_start(&e, edit)?;
                    writer.write_event(Event::Start(start))?;
                    for (q, _) in &edit.sets {
                        if let Some(frag) = edit.fragment_for(q) {
                            writer.get_mut().write_all(frag.as_bytes())?;
                        }
                        remember(&mut done, q.clone());
                    }
                    writer.write_event(Event::End(BytesEnd::new("rdf:Description")))?;
                } else if in_desc && child_depth == 0 && edit.is_managed(&name) {
                    if let Some(frag) = edit.fragment_for(&name) {
                        writer.get_mut().write_all(frag.as_bytes())?;
                    }
                    remember(&mut done, name);
                } else {
                    writer.write_event(Event::Empty(e))?;
                }
            }
            Ok(Event::End(e)) => {
                let name = qname_string(e.name());
                if in_desc && child_depth == 0 && name == "rdf:Description" {
                    // 关闭目标 Description 前，补写尚未出现的 set 属性
                    for (q, _) in &edit.sets {
                        if !done.iter().any(|d| d == q)
                            && let Some(frag) = edit.fragment_for(q)
                        {
                            writer.get_mut().write_all(b"\n   ")?;
                            writer.get_mut().write_all(frag.as_bytes())?;
                        }
                    }
                    in_desc = false;
                    writer.write_event(Event::End(e))?;
                } else {
                    if in_desc {
                        child_depth -= 1;
                    }
                    writer.write_event(Event::End(e))?;
                }
            }
            Ok(Event::Eof) => break,
            Ok(ev) => {
                writer.write_event(ev)?;
            }
            Err(e) => bail!("解析 XMP 失败：{e}"),
        }
    }

    let bytes = writer.into_inner();
    Ok(String::from_utf8(bytes)?)
}

/// 重建 rdf:Description 的起始标签：保留 xmlns/rdf:about 与未管理的属性形式属性，
/// 丢弃被管理的属性形式属性（改以元素形式写），并补齐所需 xmlns 声明。
fn rebuild_desc_start(e: &BytesStart, edit: &XmpEdit) -> Result<BytesStart<'static>> {
    let mut start = BytesStart::new("rdf:Description");
    let mut declared: Vec<String> = Vec::new();

    for attr in e.attributes() {
        let attr = attr.map_err(|err| anyhow::anyhow!("XMP 属性解析失败：{err}"))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let value = attr
            .unescape_value()
            .map_err(|err| anyhow::anyhow!("XMP 属性解码失败：{err}"))?;

        if key.starts_with("xmlns:") || key == "xmlns" {
            declared.push(key.trim_start_matches("xmlns:").to_string());
            start.push_attribute((key.as_str(), value.as_ref()));
        } else if key == "rdf:about" {
            start.push_attribute((key.as_str(), value.as_ref()));
        } else if edit.is_managed(&key) {
            // 丢弃：改以元素形式写入或删除
        } else {
            // 保留未知的属性形式属性
            start.push_attribute((key.as_str(), value.as_ref()));
        }
    }

    // 补齐本次会用到、但尚未声明的命名空间
    for prefix in edit.used_prefixes() {
        if !declared.iter().any(|d| d == prefix)
            && let Some(uri) = namespace_uri(prefix)
        {
            start.push_attribute((format!("xmlns:{prefix}").as_str(), uri));
        }
    }

    Ok(start.into_owned())
}

// ============================ 读取（show）============================

/// 解析 XMP 包，返回 (限定名, 显示值) 列表。
pub fn read_properties(packet: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut reader = Reader::from_str(packet);
    reader.config_mut().check_end_names = false;

    let mut in_desc = false;
    let mut child_depth: i32 = 0;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = qname_string(e.name());
                if name == "rdf:Description" {
                    in_desc = true;
                    child_depth = 0;
                    collect_attr_props(&e, &mut out);
                } else if in_desc && child_depth == 0 {
                    // 一个属性元素：读取其内部所有文本
                    let end = e.to_end().into_owned();
                    let span_ok = reader.read_to_end(end.name());
                    match span_ok {
                        Ok(span) => {
                            let inner = &packet[span.start as usize..span.end as usize];
                            out.push((name, extract_text(inner)));
                        }
                        Err(_) => break,
                    }
                } else if in_desc {
                    child_depth += 1;
                }
            }
            Ok(Event::Empty(e)) => {
                let name = qname_string(e.name());
                if name == "rdf:Description" {
                    // 自闭合 Description：属性即全部属性形式的属性
                    collect_attr_props(&e, &mut out);
                } else if in_desc && child_depth == 0 {
                    out.push((name, String::new()));
                }
            }
            Ok(Event::End(e)) => {
                let name = qname_string(e.name());
                if in_desc && child_depth == 0 && name == "rdf:Description" {
                    in_desc = false;
                } else if in_desc {
                    child_depth -= 1;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    out
}

fn collect_attr_props(e: &BytesStart, out: &mut Vec<(String, String)>) {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        if key.starts_with("xmlns") || key == "rdf:about" {
            continue;
        }
        if let Ok(v) = attr.unescape_value() {
            out.push((key, v.to_string()));
        }
    }
}

/// 从元素内部 XML 中抽取所有文本，多个 rdf:li 用 "; " 连接。
fn extract_text(inner: &str) -> String {
    let mut reader = Reader::from_str(inner);
    reader.config_mut().check_end_names = false;
    let mut parts: Vec<String> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(t)) => {
                if let Ok(s) = t.unescape() {
                    let s = s.trim();
                    if !s.is_empty() {
                        parts.push(s.to_string());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    parts.join("; ")
}

// ============================ JPEG 段手术 ============================

/// 在 JPEG 字节流中查找 XMP APP1 段，返回 (段起始, 段结束(不含), 包字节)。
pub fn find_jpeg_xmp(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF {
            break;
        }
        let marker = bytes[i + 1];
        // 无长度字段的标记
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if marker == 0xDA {
            break; // 到达图像扫描数据
        }
        let len = ((bytes[i + 2] as usize) << 8) | bytes[i + 3] as usize;
        if len < 2 || i + 2 + len > bytes.len() {
            break;
        }
        let payload = &bytes[i + 4..i + 2 + len];
        if marker == 0xE1 && payload.starts_with(XMP_SIG) {
            let packet = payload[XMP_SIG.len()..].to_vec();
            return Some((i, i + 2 + len, packet));
        }
        i += 2 + len;
    }
    None
}

/// 设置（替换或插入）JPEG 的 XMP 段。
pub fn set_jpeg_xmp(bytes: &mut Vec<u8>, packet: &str) -> Result<()> {
    let content_len = XMP_SIG.len() + packet.len();
    if content_len + 2 > 0xFFFF {
        bail!(
            "XMP 数据过大（{} 字节），暂不支持扩展 XMP 分段写入",
            packet.len()
        );
    }
    let seg_len = (content_len + 2) as u16;
    let mut segment = Vec::with_capacity(content_len + 4);
    segment.push(0xFF);
    segment.push(0xE1);
    segment.push((seg_len >> 8) as u8);
    segment.push((seg_len & 0xFF) as u8);
    segment.extend_from_slice(XMP_SIG);
    segment.extend_from_slice(packet.as_bytes());

    if let Some((start, end, _)) = find_jpeg_xmp(bytes) {
        bytes.splice(start..end, segment);
    } else {
        // 插到所有前导 APPn 段之后，保证 EXIF 段仍是第一个 APP1
        let at = jpeg_app_insert_pos(bytes);
        bytes.splice(at..at, segment);
    }
    Ok(())
}

/// 删除 JPEG 的 XMP 段，返回是否删除了内容。
pub fn remove_jpeg_xmp(bytes: &mut Vec<u8>) -> bool {
    if let Some((start, end, _)) = find_jpeg_xmp(bytes) {
        bytes.drain(start..end);
        true
    } else {
        false
    }
}

// ============================ PNG 段手术（iTXt）============================

const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
/// PNG 中存放 XMP 的 iTXt chunk 关键字。
const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";

/// PNG 数据块的 CRC-32（多项式 0xEDB88320）。
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn be32(b: &[u8]) -> usize {
    ((b[0] as usize) << 24) | ((b[1] as usize) << 16) | ((b[2] as usize) << 8) | b[3] as usize
}

pub fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == PNG_SIG
}

pub fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8
}

/// JPEG 中所有前导 APPn 段之后的插入位置。
///
/// 关键：新增的元数据段必须插在 EXIF(APP1) 段**之后**——否则部分读取器
/// （包括 little_exif）会把第一个 APP1 当成 EXIF，读到 XMP 就失败，导致 EXIF 丢失。
pub fn jpeg_app_insert_pos(bytes: &[u8]) -> usize {
    if !is_jpeg(bytes) {
        return 2.min(bytes.len());
    }
    let mut i = 2;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF {
            break;
        }
        let marker = bytes[i + 1];
        if (0xE0..=0xEF).contains(&marker) {
            let len = ((bytes[i + 2] as usize) << 8) | bytes[i + 3] as usize;
            if len < 2 || i + 2 + len > bytes.len() {
                break;
            }
            i += 2 + len;
        } else {
            break;
        }
    }
    i
}

/// 在 PNG 中查找 XMP iTXt chunk，返回 (chunk 起始, chunk 结束(不含), 包字节)。
pub fn find_png_xmp(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    if !is_png(bytes) {
        return None;
    }
    let mut pos = 8;
    while pos + 8 <= bytes.len() {
        let len = be32(&bytes[pos..pos + 4]);
        let ctype = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        let chunk_end = data_start + len + 4; // + CRC
        if chunk_end > bytes.len() {
            break;
        }
        if ctype == b"iTXt" {
            let data = &bytes[data_start..data_start + len];
            if let Some(text) = parse_itxt_xmp(data) {
                return Some((pos, chunk_end, text));
            }
        }
        if ctype == b"IEND" {
            break;
        }
        pos = chunk_end;
    }
    None
}

/// 解析 iTXt 数据，若关键字是 XMP 则返回其文本字节。
fn parse_itxt_xmp(data: &[u8]) -> Option<Vec<u8>> {
    // keyword \0 comp_flag comp_method lang \0 transkw \0 text
    let kw_end = data.iter().position(|&b| b == 0)?;
    if &data[..kw_end] != XMP_KEYWORD {
        return None;
    }
    let mut i = kw_end + 1;
    if i + 2 > data.len() {
        return None;
    }
    let comp_flag = data[i];
    i += 2; // 跳过压缩标志与方法
    // 未压缩才处理
    if comp_flag != 0 {
        return None;
    }
    // 跳过 language tag（以 \0 结尾）
    let lang_end = data[i..].iter().position(|&b| b == 0)? + i;
    i = lang_end + 1;
    // 跳过 translated keyword（以 \0 结尾）
    let tk_end = data[i..].iter().position(|&b| b == 0)? + i;
    i = tk_end + 1;
    Some(data[i..].to_vec())
}

/// 设置（替换或插入）PNG 的 XMP iTXt chunk。
pub fn set_png_xmp(bytes: &mut Vec<u8>, packet: &str) -> Result<()> {
    // 组装 iTXt 数据
    let mut data = Vec::new();
    data.extend_from_slice(XMP_KEYWORD);
    data.push(0); // keyword 结束
    data.push(0); // compression flag = 0
    data.push(0); // compression method = 0
    data.push(0); // language tag（空）结束
    data.push(0); // translated keyword（空）结束
    data.extend_from_slice(packet.as_bytes());

    // 组装完整 chunk：len + "iTXt" + data + crc
    let mut chunk = Vec::with_capacity(data.len() + 12);
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut type_and_data = Vec::with_capacity(data.len() + 4);
    type_and_data.extend_from_slice(b"iTXt");
    type_and_data.extend_from_slice(&data);
    chunk.extend_from_slice(&type_and_data);
    chunk.extend_from_slice(&crc32(&type_and_data).to_be_bytes());

    if let Some((start, end, _)) = find_png_xmp(bytes) {
        bytes.splice(start..end, chunk);
    } else {
        let at = png_insert_pos(bytes)?;
        bytes.splice(at..at, chunk);
    }
    Ok(())
}

/// 删除 PNG 的 XMP iTXt chunk。
pub fn remove_png_xmp(bytes: &mut Vec<u8>) -> bool {
    if let Some((start, end, _)) = find_png_xmp(bytes) {
        bytes.drain(start..end);
        true
    } else {
        false
    }
}

/// XMP chunk 的插入位置：第一个 IDAT 之前（否则 IEND 之前）。
fn png_insert_pos(bytes: &[u8]) -> Result<usize> {
    let mut pos = 8;
    let mut iend: Option<usize> = None;
    while pos + 8 <= bytes.len() {
        let len = be32(&bytes[pos..pos + 4]);
        let ctype = &bytes[pos + 4..pos + 8];
        let chunk_end = pos + 12 + len;
        if chunk_end > bytes.len() {
            break;
        }
        if ctype == b"IDAT" {
            return Ok(pos);
        }
        if ctype == b"IEND" {
            iend = Some(pos);
            break;
        }
        pos = chunk_end;
    }
    iend.ok_or_else(|| anyhow::anyhow!("PNG 结构异常：未找到 IDAT/IEND"))
}

// ============================ 容器分发 ============================

/// 从 JPEG 或 PNG 中取出 XMP 包的原始字节。
pub fn extract_packet_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    if is_jpeg(bytes) {
        find_jpeg_xmp(bytes).map(|(_, _, p)| p)
    } else if is_png(bytes) {
        find_png_xmp(bytes).map(|(_, _, p)| p)
    } else {
        None
    }
}

/// 把 XMP 包写入 JPEG 或 PNG（替换或插入）。
pub fn write_packet(bytes: &mut Vec<u8>, packet: &str) -> Result<()> {
    if is_jpeg(bytes) {
        set_jpeg_xmp(bytes, packet)
    } else if is_png(bytes) {
        set_png_xmp(bytes, packet)
    } else {
        bail!("XMP 目前仅支持 JPEG 与 PNG")
    }
}

/// 删除 JPEG 或 PNG 的 XMP。
pub fn remove_packet(bytes: &mut Vec<u8>) -> bool {
    if is_jpeg(bytes) {
        remove_jpeg_xmp(bytes)
    } else if is_png(bytes) {
        remove_png_xmp(bytes)
    } else {
        false
    }
}

/// 该容器是否支持 XMP。
pub fn supports_xmp(bytes: &[u8]) -> bool {
    is_jpeg(bytes) || is_png(bytes)
}

// ============================ Sidecar (.xmp) ============================

/// 某文件对应的 sidecar XMP 写入路径（`<主干>.xmp`，Adobe/Lightroom 约定）。
/// 例：`photo.CR2` → `photo.xmp`。
pub fn sidecar_path(image: &Path) -> std::path::PathBuf {
    image.with_extension("xmp")
}

/// 读取 sidecar XMP 文本：先查 `<主干>.xmp`，再查 `<全名>.xmp`（exiftool 约定）。
pub fn read_sidecar(image: &Path) -> Option<String> {
    let mut candidates = vec![image.with_extension("xmp")];
    if let Some(name) = image.file_name() {
        let mut n = name.to_os_string();
        n.push(".xmp");
        candidates.push(image.with_file_name(n));
    }
    for p in candidates {
        if p.is_file()
            && let Ok(s) = std::fs::read_to_string(&p)
        {
            return Some(s.strip_prefix('\u{feff}').unwrap_or(&s).to_string());
        }
    }
    None
}

fn qname_string(q: QName) -> String {
    String::from_utf8_lossy(q.as_ref()).to_string()
}

fn remember(done: &mut Vec<String>, name: String) {
    if !done.contains(&name) {
        done.push(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_edit() -> XmpEdit {
        XmpEdit {
            sets: vec![
                ("xmp:Rating".into(), XmpValue::Simple("5".into())),
                (
                    "dc:creator".into(),
                    XmpValue::Seq(vec!["张三".into(), "李四".into()]),
                ),
            ],
            removes: vec![],
        }
    }

    #[test]
    fn fresh_packet_contains_values() {
        let pkt = apply(None, &simple_edit()).unwrap();
        assert!(pkt.contains("<xmp:Rating>5</xmp:Rating>"));
        assert!(pkt.contains("<rdf:li>张三</rdf:li>"));
        assert!(pkt.contains("xmlns:dc="));
        assert!(pkt.contains("xmlns:xmp="));
        // 可被再次解析
        let props = read_properties(&pkt);
        assert!(props.iter().any(|(k, v)| k == "xmp:Rating" && v == "5"));
        assert!(
            props
                .iter()
                .any(|(k, v)| k == "dc:creator" && v == "张三; 李四")
        );
    }

    #[test]
    fn edit_preserves_unknown_props() {
        let original = "<?xpacket begin=\"\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
            <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
            <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
            xmlns:custom=\"http://example.com/\">\
            <custom:Secret>keepme</custom:Secret>\
            <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Old</rdf:li></rdf:Alt></dc:title>\
            </rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>";
        let edit = XmpEdit {
            sets: vec![("dc:title".into(), XmpValue::LangAlt("New".into()))],
            removes: vec![],
        };
        let out = apply(Some(original), &edit).unwrap();
        // 未知属性保留
        assert!(out.contains("<custom:Secret>keepme</custom:Secret>"));
        // 标题被替换
        assert!(out.contains(">New</rdf:li>"));
        assert!(!out.contains(">Old</rdf:li>"));
    }

    #[test]
    fn remove_attribute_form_property() {
        // xmp:Rating 以属性形式存在，remove 后应消失
        let original = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
            <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
            <rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmp:Rating=\"3\"/>\
            </rdf:RDF></x:xmpmeta>";
        let edit = XmpEdit {
            sets: vec![],
            removes: vec!["xmp:Rating".into()],
        };
        let out = apply(Some(original), &edit).unwrap();
        assert!(!out.contains("Rating=\"3\""));
        assert!(!out.contains("xmp:Rating"));
    }

    #[test]
    fn replace_attribute_form_with_element() {
        let original = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
            <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
            <rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmp:Rating=\"3\"/>\
            </rdf:RDF></x:xmpmeta>";
        let edit = XmpEdit {
            sets: vec![("xmp:Rating".into(), XmpValue::Simple("5".into()))],
            removes: vec![],
        };
        let out = apply(Some(original), &edit).unwrap();
        assert!(!out.contains("Rating=\"3\""));
        assert!(out.contains("<xmp:Rating>5</xmp:Rating>"));
    }

    #[test]
    fn png_itxt_roundtrip() {
        // 最小 PNG：签名 + IHDR + IDAT + IEND
        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIG);
        let chunk = |ctype: &[u8], data: &[u8], out: &mut Vec<u8>| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut td = Vec::new();
            td.extend_from_slice(ctype);
            td.extend_from_slice(data);
            out.extend_from_slice(&td);
            out.extend_from_slice(&crc32(&td).to_be_bytes());
        };
        chunk(b"IHDR", &[0; 13], &mut png);
        chunk(b"IDAT", &[1, 2, 3], &mut png);
        chunk(b"IEND", &[], &mut png);
        let idat_before = png.windows(4).filter(|w| *w == b"IDAT").count();

        assert!(find_png_xmp(&png).is_none());
        set_png_xmp(&mut png, "<x>hi</x>").unwrap();
        let (_, _, packet) = find_png_xmp(&png).unwrap();
        assert_eq!(packet, b"<x>hi</x>");
        // XMP 应插在 IDAT 之前，且 IDAT 仍在
        assert_eq!(
            png.windows(4).filter(|w| *w == b"IDAT").count(),
            idat_before
        );
        assert!(remove_png_xmp(&mut png));
        assert!(find_png_xmp(&png).is_none());
        assert_eq!(&png[0..8], &PNG_SIG);
    }

    #[test]
    fn jpeg_segment_roundtrip() {
        // 最小 JPEG：SOI + 一个 APP0 + EOI
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xD9];
        assert!(find_jpeg_xmp(&jpeg).is_none());
        set_jpeg_xmp(&mut jpeg, "<x>hi</x>").unwrap();
        let (_, _, packet) = find_jpeg_xmp(&jpeg).unwrap();
        assert_eq!(packet, b"<x>hi</x>");
        assert!(remove_jpeg_xmp(&mut jpeg));
        assert!(find_jpeg_xmp(&jpeg).is_none());
        // SOI 与 EOI 仍在
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8]);
    }
}

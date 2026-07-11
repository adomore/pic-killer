//! GPX 轨迹解析与「按时间插值定位」。
//!
//! 解析 `<trkpt>` / `<rtept>` / `<wpt>` 的经纬度、时间(可选海拔)，按时间排序；
//! 给定某一 UTC 时刻，返回该时刻在轨迹上的插值坐标（超出 `max_gap` 则返回 None）。

use std::path::Path;

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// 一个轨迹点。
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPoint {
    pub time: DateTime<Utc>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
}

/// 解析 GPX 文件，返回按时间升序排列的轨迹点（丢弃没有时间戳的点）。
pub fn parse(path: &Path) -> Result<Vec<TrackPoint>> {
    let data = std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("读取 GPX 失败：{e}"))?;
    parse_str(&data)
}

fn parse_str(data: &str) -> Result<Vec<TrackPoint>> {
    let mut reader = Reader::from_str(data);
    reader.config_mut().check_end_names = false;

    let mut points = Vec::new();
    let mut lat: Option<f64> = None;
    let mut lon: Option<f64> = None;
    let mut ele: Option<f64> = None;
    let mut time: Option<DateTime<Utc>> = None;
    let mut in_point = false;
    let mut text_target: Option<Field> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.local_name();
                let name = name.as_ref();
                if is_point_tag(name) {
                    in_point = true;
                    lat = None;
                    lon = None;
                    ele = None;
                    time = None;
                    for attr in e.attributes().flatten() {
                        let key = attr.key.local_name();
                        let val = attr.unescape_value().ok();
                        match key.as_ref() {
                            b"lat" => lat = val.and_then(|v| v.parse().ok()),
                            b"lon" => lon = val.and_then(|v| v.parse().ok()),
                            _ => {}
                        }
                    }
                } else if in_point && name == b"time" {
                    text_target = Some(Field::Time);
                } else if in_point && name == b"ele" {
                    text_target = Some(Field::Ele);
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(field) = text_target.take() {
                    let raw = t.unescape().unwrap_or_default();
                    let s = raw.trim();
                    match field {
                        Field::Time => {
                            time = DateTime::parse_from_rfc3339(s)
                                .ok()
                                .map(|d| d.with_timezone(&Utc));
                        }
                        Field::Ele => ele = s.parse().ok(),
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                if is_point_tag(name.as_ref()) {
                    if let (Some(la), Some(lo), Some(ti)) = (lat, lon, time) {
                        points.push(TrackPoint {
                            time: ti,
                            lat: la,
                            lon: lo,
                            ele,
                        });
                    }
                    in_point = false;
                }
                text_target = None;
            }
            Ok(Event::Eof) => break,
            Err(e) => bail!("解析 GPX 失败：{e}"),
            _ => {}
        }
    }

    points.sort_by_key(|p| p.time);
    Ok(points)
}

enum Field {
    Time,
    Ele,
}

fn is_point_tag(name: &[u8]) -> bool {
    matches!(name, b"trkpt" | b"rtept" | b"wpt")
}

/// 在轨迹上定位某个 UTC 时刻的坐标，返回 (纬度, 经度, 海拔)。
///
/// - 时刻正好落在两点之间：线性插值。
/// - 时刻在轨迹端点之外但相差不超过 `max_gap` 秒：贴到端点。
/// - 相邻两点间隔超过 `max_gap`：只有贴近某一点时才返回，否则 None（不在间隙里瞎插）。
pub fn locate(
    points: &[TrackPoint],
    t: DateTime<Utc>,
    max_gap: i64,
) -> Option<(f64, f64, Option<f64>)> {
    if points.is_empty() {
        return None;
    }
    match points.binary_search_by(|p| p.time.cmp(&t)) {
        Ok(i) => {
            let p = &points[i];
            Some((p.lat, p.lon, p.ele))
        }
        Err(i) => {
            if i == 0 {
                let n = &points[0];
                within(n, (n.time - t).num_seconds().abs(), max_gap)
            } else if i == points.len() {
                let p = &points[points.len() - 1];
                within(p, (t - p.time).num_seconds().abs(), max_gap)
            } else {
                let prev = &points[i - 1];
                let next = &points[i];
                let gap = (next.time - prev.time).num_seconds();
                let dp = (t - prev.time).num_seconds();
                let dn = (next.time - t).num_seconds();
                if gap <= 0 {
                    return Some((prev.lat, prev.lon, prev.ele));
                }
                if gap > max_gap {
                    // 间隙过大：只贴到相差在 max_gap 内的那个端点
                    if dp <= dn && dp <= max_gap {
                        Some((prev.lat, prev.lon, prev.ele))
                    } else if dn <= max_gap {
                        Some((next.lat, next.lon, next.ele))
                    } else {
                        None
                    }
                } else {
                    let frac = dp as f64 / gap as f64;
                    let lat = prev.lat + (next.lat - prev.lat) * frac;
                    let lon = prev.lon + (next.lon - prev.lon) * frac;
                    let ele = match (prev.ele, next.ele) {
                        (Some(a), Some(b)) => Some(a + (b - a) * frac),
                        _ => None,
                    };
                    Some((lat, lon, ele))
                }
            }
        }
    }
}

fn within(p: &TrackPoint, dist: i64, max_gap: i64) -> Option<(f64, f64, Option<f64>)> {
    if dist <= max_gap {
        Some((p.lat, p.lon, p.ele))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><trkseg>
    <trkpt lat="30.0" lon="120.0"><ele>10</ele><time>2023-01-01T10:00:00Z</time></trkpt>
    <trkpt lat="30.2" lon="120.2"><ele>20</ele><time>2023-01-01T10:10:00Z</time></trkpt>
    <trkpt lat="31.0" lon="121.0"><time>2023-01-01T12:00:00Z</time></trkpt>
  </trkseg></trk>
</gpx>"#;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn parses_points_sorted() {
        let pts = parse_str(SAMPLE).unwrap();
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0].lat, 30.0);
        assert_eq!(pts[0].ele, Some(10.0));
        assert_eq!(pts[2].ele, None);
        assert!(pts[0].time < pts[1].time);
    }

    #[test]
    fn exact_point() {
        let pts = parse_str(SAMPLE).unwrap();
        let r = locate(&pts, t("2023-01-01T10:00:00Z"), 600).unwrap();
        assert_eq!(r.0, 30.0);
        assert_eq!(r.1, 120.0);
    }

    #[test]
    fn interpolates_midway() {
        let pts = parse_str(SAMPLE).unwrap();
        // 10:05 正好在 10:00 与 10:10 中点 → 纬度 30.1、经度 120.1、海拔 15
        let (lat, lon, ele) = locate(&pts, t("2023-01-01T10:05:00Z"), 600).unwrap();
        assert!((lat - 30.1).abs() < 1e-9);
        assert!((lon - 120.1).abs() < 1e-9);
        assert!((ele.unwrap() - 15.0).abs() < 1e-9);
    }

    #[test]
    fn large_gap_snaps_or_skips() {
        let pts = parse_str(SAMPLE).unwrap();
        // 10:10 与 12:00 间隔 110 分钟 > max_gap(600s)。
        // 10:12（距 10:10 仅 120s）应贴到前点。
        let near = locate(&pts, t("2023-01-01T10:12:00Z"), 600).unwrap();
        assert_eq!(near.0, 30.2);
        // 11:00 距两端都 > 600s → None。
        assert!(locate(&pts, t("2023-01-01T11:00:00Z"), 600).is_none());
    }

    #[test]
    fn outside_track_within_gap() {
        let pts = parse_str(SAMPLE).unwrap();
        // 09:59:30 在首点前 30s，max_gap=600 → 贴首点
        assert!(locate(&pts, t("2023-01-01T09:59:30Z"), 600).is_some());
        // 09:00 在首点前 1h > 600s → None
        assert!(locate(&pts, t("2023-01-01T09:00:00Z"), 600).is_none());
    }
}

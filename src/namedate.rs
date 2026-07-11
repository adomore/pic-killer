//! 从文件名中提取拍摄日期时间。
//!
//! 覆盖常见相机 / 手机 / 截图命名，如：
//! - `IMG_20230115_143022.jpg`
//! - `20230115_143022.jpg` / `20230115143022.jpg`
//! - `PXL_20230115_143022123.jpg`（Pixel，带毫秒）
//! - `2023-01-15 14.30.22.jpg`
//! - `Screenshot_2023-01-15-14-30-22.png`
//! - `2023-01-15.jpg`（仅日期，时间按 00:00:00）
//!
//! 策略：抽取文件名主干中的全部数字，寻找第一个能构成合法日期(YYYYMMDD)的位置，
//! 若其后紧跟 6 位则解析为时间(HHMMSS)。这样无论中间用什么分隔符都能识别。

use chrono::NaiveDateTime;

/// 从文件名（不含扩展名的主干即可，传整个文件名也行）提取日期时间。
pub fn extract(file_stem: &str) -> Option<NaiveDateTime> {
    // 记录每个数字字符，忽略其它字符（分隔符）
    let digits: Vec<u8> = file_stem.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
    if digits.len() < 8 {
        return None;
    }

    // 尝试每个起点，找第一个合法日期
    for start in 0..=(digits.len() - 8) {
        let year = num(&digits[start..start + 4]);
        let month = num(&digits[start + 4..start + 6]) as u32;
        let day = num(&digits[start + 6..start + 8]) as u32;

        if !(1970..=2099).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            continue;
        }

        // 是否还有 6 位可作为时间
        let (hh, mm, ss) = if start + 14 <= digits.len() {
            let h = num(&digits[start + 8..start + 10]) as u32;
            let mi = num(&digits[start + 10..start + 12]) as u32;
            let s = num(&digits[start + 12..start + 14]) as u32;
            if h < 24 && mi < 60 && s < 60 {
                (h, mi, s)
            } else {
                (0, 0, 0)
            }
        } else {
            (0, 0, 0)
        };

        if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
            if let Some(dt) = date.and_hms_opt(hh, mm, ss) {
                return Some(dt);
            }
        }
    }
    None
}

fn num(digits: &[u8]) -> i32 {
    digits.iter().fold(0i32, |acc, &d| acc * 10 + d as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(s: &str) -> Option<String> {
        extract(s).map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
    }

    #[test]
    fn common_camera_names() {
        assert_eq!(fmt("IMG_20230115_143022").as_deref(), Some("2023-01-15 14:30:22"));
        assert_eq!(fmt("20230115_143022").as_deref(), Some("2023-01-15 14:30:22"));
        assert_eq!(fmt("20230115143022").as_deref(), Some("2023-01-15 14:30:22"));
        assert_eq!(fmt("PXL_20230115_143022123").as_deref(), Some("2023-01-15 14:30:22"));
        assert_eq!(fmt("2023-01-15 14.30.22").as_deref(), Some("2023-01-15 14:30:22"));
        assert_eq!(fmt("Screenshot_2023-01-15-14-30-22").as_deref(), Some("2023-01-15 14:30:22"));
    }

    #[test]
    fn date_only() {
        assert_eq!(fmt("2023-01-15").as_deref(), Some("2023-01-15 00:00:00"));
        assert_eq!(fmt("photo_20230115").as_deref(), Some("2023-01-15 00:00:00"));
    }

    #[test]
    fn leading_number_before_date() {
        // 前导序号不应干扰：1_20230115_143022
        assert_eq!(fmt("1_20230115_143022").as_deref(), Some("2023-01-15 14:30:22"));
    }

    #[test]
    fn invalid_time_falls_back_to_midnight() {
        // 99:99:99 非法 → 退回 00:00:00
        assert_eq!(fmt("20230115_999999").as_deref(), Some("2023-01-15 00:00:00"));
    }

    #[test]
    fn no_date_returns_none() {
        assert_eq!(fmt("IMG_WA0001"), None);
        assert_eq!(fmt("vacation"), None);
        assert_eq!(fmt("12345"), None);
    }
}

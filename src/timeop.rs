//! 时间解析与运算：绝对时间解析、相对偏移量（Delta）解析、EXIF 时间格式化。

use anyhow::{Context, Result, bail};
use chrono::{Duration, Months, NaiveDateTime};

/// EXIF 时间字段的标准格式：`YYYY:MM:DD HH:MM:SS`
pub const EXIF_FMT: &str = "%Y:%m:%d %H:%M:%S";

/// 相对偏移量。月/年无法换算成固定秒数，因此单独保存。
/// 字段已包含符号：负值表示往前调。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delta {
    pub months: i64,
    pub days: i64,
    pub seconds: i64,
}

impl Delta {
    /// 把偏移量应用到某个时间上。月份用日历运算（自动处理月末），其余用固定时长。
    pub fn apply(&self, dt: NaiveDateTime) -> Option<NaiveDateTime> {
        let mut r = dt;
        if self.months > 0 {
            r = r.checked_add_months(Months::new(self.months as u32))?;
        } else if self.months < 0 {
            r = r.checked_sub_months(Months::new((-self.months) as u32))?;
        }
        r.checked_add_signed(Duration::days(self.days) + Duration::seconds(self.seconds))
    }

    /// 按倍数缩放（用于序列模式：第 n 张 = 起始 + n × 间隔）。
    pub fn scaled(&self, n: i64) -> Delta {
        Delta {
            months: self.months * n,
            days: self.days * n,
            seconds: self.seconds * n,
        }
    }

    /// 是否为零偏移。
    pub fn is_zero(&self) -> bool {
        self.months == 0 && self.days == 0 && self.seconds == 0
    }
}

/// 解析相对偏移量，如 `+2h`、`-3d`、`+1y2mo`、`-30m`、`+1d12h30m`。
///
/// 单位（大小写不敏感）：
/// - `y` / `yr` / `year(s)` — 年
/// - `mo` / `month(s)`      — 月
/// - `w` / `week(s)`        — 周
/// - `d` / `day(s)`         — 天
/// - `h` / `hr` / `hour(s)` — 时
/// - `m` / `min` / `minute(s)` — 分（注意与 `mo` 区分）
/// - `s` / `sec` / `second(s)` — 秒
pub fn parse_delta(input: &str) -> Result<Delta> {
    let s = input.trim();
    if s.is_empty() {
        bail!("偏移量为空");
    }

    // 解析前导符号
    let (negative, rest) = if let Some(r) = s.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (false, r)
    } else {
        (false, s)
    };

    let mut months: i64 = 0;
    let mut days: i64 = 0;
    let mut seconds: i64 = 0;

    let mut chars = rest.chars().peekable();
    let mut saw_any = false;

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        // 读取数字
        let mut num = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                num.push(d);
                chars.next();
            } else {
                break;
            }
        }
        if num.is_empty() {
            bail!("偏移量 `{input}` 格式错误：单位 `{c}` 前缺少数字");
        }
        let val: i64 = num.parse().with_context(|| format!("数字过大：{num}"))?;

        // 读取单位字母
        let mut unit = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_alphabetic() {
                unit.push(d.to_ascii_lowercase());
                chars.next();
            } else {
                break;
            }
        }

        match unit.as_str() {
            "y" | "yr" | "yrs" | "year" | "years" => months += val * 12,
            "mo" | "mon" | "month" | "months" => months += val,
            "w" | "wk" | "week" | "weeks" => days += val * 7,
            "d" | "day" | "days" => days += val,
            "h" | "hr" | "hrs" | "hour" | "hours" => seconds += val * 3600,
            "m" | "min" | "mins" | "minute" | "minutes" => seconds += val * 60,
            "s" | "sec" | "secs" | "second" | "seconds" => seconds += val,
            "" => bail!("偏移量 `{input}` 格式错误：数字 `{val}` 后缺少单位"),
            other => bail!("偏移量 `{input}` 含未知单位 `{other}`"),
        }
        saw_any = true;
    }

    if !saw_any {
        bail!("偏移量 `{input}` 未包含任何有效的数值-单位组合");
    }

    let sign = if negative { -1 } else { 1 };
    Ok(Delta {
        months: months * sign,
        days: days * sign,
        seconds: seconds * sign,
    })
}

/// 解析绝对时间。接受多种常见写法，日期分隔符可用 `-` 或 `:`，
/// 可省略秒或整个时间部分（省略时按 00:00:00 处理）。
pub fn parse_datetime(input: &str) -> Result<NaiveDateTime> {
    let s = input.trim().trim_end_matches('\0').trim();

    // 先按“完整日期时间”尝试各种格式
    const DATETIME_FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S",
        "%Y:%m:%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y:%m:%d %H:%M",
        "%Y/%m/%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ];
    for fmt in DATETIME_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(dt);
        }
    }

    // 再按“仅日期”尝试，补足 00:00:00
    const DATE_FORMATS: &[&str] = &["%Y-%m-%d", "%Y:%m:%d", "%Y/%m/%d"];
    for fmt in DATE_FORMATS {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, fmt) {
            return Ok(d.and_hms_opt(0, 0, 0).expect("00:00:00 合法"));
        }
    }

    bail!("无法解析时间 `{input}`，请用形如 `2024-01-01 12:00:00` 的格式");
}

/// 把时间格式化为 EXIF 标准字符串 `YYYY:MM:DD HH:MM:SS`。
pub fn format_exif(dt: &NaiveDateTime) -> String {
    dt.format(EXIF_FMT).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        parse_datetime(s).unwrap()
    }

    #[test]
    fn delta_basic() {
        assert_eq!(
            parse_delta("+2h").unwrap(),
            Delta {
                months: 0,
                days: 0,
                seconds: 7200
            }
        );
        assert_eq!(
            parse_delta("-3d").unwrap(),
            Delta {
                months: 0,
                days: -3,
                seconds: 0
            }
        );
        assert_eq!(
            parse_delta("30m").unwrap(),
            Delta {
                months: 0,
                days: 0,
                seconds: 1800
            }
        );
    }

    #[test]
    fn delta_compound_and_month_vs_minute() {
        // 1y2mo3d4h5m6s
        let d = parse_delta("+1y2mo3d4h5m6s").unwrap();
        assert_eq!(d.months, 14);
        assert_eq!(d.days, 3);
        assert_eq!(d.seconds, 4 * 3600 + 5 * 60 + 6);
        // `m` 是分钟，`mo` 是月
        assert_eq!(parse_delta("5m").unwrap().seconds, 300);
        assert_eq!(parse_delta("5mo").unwrap().months, 5);
    }

    #[test]
    fn delta_negative_sign_applies_to_all() {
        let d = parse_delta("-1d12h").unwrap();
        assert_eq!(d.days, -1);
        assert_eq!(d.seconds, -12 * 3600);
    }

    #[test]
    fn delta_apply_month_end() {
        // 1月31日 + 1个月 = 2月28日（日历运算）
        let r = parse_delta("+1mo")
            .unwrap()
            .apply(dt("2023-01-31 10:00:00"))
            .unwrap();
        assert_eq!(format_exif(&r), "2023:02:28 10:00:00");
    }

    #[test]
    fn delta_apply_seconds_cross_day() {
        let r = parse_delta("+2h")
            .unwrap()
            .apply(dt("2024-01-01 23:00:00"))
            .unwrap();
        assert_eq!(format_exif(&r), "2024:01:02 01:00:00");
    }

    #[test]
    fn delta_scaled_for_sequence() {
        let d = parse_delta("+10s").unwrap();
        assert_eq!(d.scaled(3).seconds, 30);
    }

    #[test]
    fn parse_various_datetime_formats() {
        assert_eq!(
            format_exif(&dt("2024-01-02 03:04:05")),
            "2024:01:02 03:04:05"
        );
        assert_eq!(
            format_exif(&dt("2024:01:02 03:04:05")),
            "2024:01:02 03:04:05"
        );
        assert_eq!(format_exif(&dt("2024-01-02")), "2024:01:02 00:00:00");
        assert_eq!(format_exif(&dt("2024/01/02 03:04")), "2024:01:02 03:04:00");
    }

    #[test]
    fn bad_delta_rejected() {
        assert!(parse_delta("").is_err());
        assert!(parse_delta("+").is_err());
        assert!(parse_delta("5x").is_err());
        assert!(parse_delta("abc").is_err());
    }
}

use std::ffi::CStr;

pub fn fmt_bytes(n: Option<f64>, dec: usize) -> String {
    let Some(mut n) = n else {
        return "—".to_string();
    };
    if !n.is_finite() {
        return "—".to_string();
    }
    if n.abs() < 1000.0 {
        return format!("{} B", n.round() as i64);
    }
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    let mut i = 0;
    n /= 1000.0;
    while n.abs() >= 1000.0 && i < UNITS.len() - 1 {
        n /= 1000.0;
        i += 1;
    }
    let d = if n.abs() >= 100.0 {
        0
    } else if n.abs() >= 10.0 {
        1
    } else {
        dec
    };
    format!("{:.*} {}", d, n, UNITS[i])
}

pub fn fmt_rate(n: Option<f64>) -> String {
    match n {
        None => "—".to_string(),
        Some(v) => format!("{}/s", fmt_bytes(Some(v), 1)),
    }
}

pub fn fmt_pct(n: Option<f64>) -> String {
    let Some(n) = n else {
        return "—".to_string();
    };
    if n == 0.0 {
        return "0%".to_string();
    }
    if n < 10.0 {
        format!("{:.1}%", n)
    } else {
        format!("{}%", n.round() as i64)
    }
}

pub fn fmt_uptime(sec: f64) -> String {
    let sec = sec.max(0.0) as u64;
    let d = sec / 86400;
    let rem = sec % 86400;
    let h = rem / 3600;
    let rem = rem % 3600;
    let m = rem / 60;
    let s = rem % 60;
    if d > 0 {
        format!("{}d {}:{:02}:{:02}", d, h, m, s)
    } else {
        format!("{}:{:02}:{:02}", h, m, s)
    }
}

pub fn fmt_cpu_time(sec: f64) -> String {
    let sec = sec.max(0.0) as u64;
    let h = sec / 3600;
    let rem = sec % 3600;
    let m = rem / 60;
    let s = rem % 60;
    format!("{}:{:02}:{:02}", h, m, s)
}

pub fn fmt_clock(ts: f64) -> String {
    if ts <= 0.0 || !ts.is_finite() {
        return "—".to_string();
    }
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let res = unsafe { libc::localtime_r(&t, &mut tm) };
    if res.is_null() {
        return "—".to_string();
    }
    let mut buf = [0u8; 32];
    let fmt = c"%H:%M";
    let len = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            fmt.as_ptr(),
            &tm,
        )
    };
    if len == 0 {
        return "—".to_string();
    }
    unsafe { CStr::from_ptr(buf.as_ptr() as *const libc::c_char) }
        .to_string_lossy()
        .into_owned()
}

pub fn nice_label(nice: i64) -> &'static str {
    if nice <= -10 {
        "High"
    } else if nice < 0 {
        "Above normal"
    } else if nice == 0 {
        "Normal"
    } else if nice <= 10 {
        "Below normal"
    } else {
        "Low"
    }
}

pub fn nice_max(v: f64) -> f64 {
    if !v.is_finite() || v <= 0.0 {
        return 1.0;
    }
    let exp = v.log10().floor();
    let base = 10f64.powf(exp);
    let f = v / base;
    let mult = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    mult * base
}

pub fn freq_to_channel(ghz: f64) -> Option<u32> {
    if ghz <= 0.0 || !ghz.is_finite() {
        return None;
    }
    let mhz = ghz * 1000.0;
    if (2412.0..=2484.0).contains(&mhz) {
        if (mhz - 2484.0).abs() < 0.5 {
            Some(14)
        } else {
            Some(((mhz - 2407.0) / 5.0) as u32)
        }
    } else if (5000.0..=5925.0).contains(&mhz) {
        Some(((mhz - 5000.0) / 5.0) as u32)
    } else if (5955.0..=7115.0).contains(&mhz) {
        Some(((mhz - 5950.0) / 5.0) as u32)
    } else {
        None
    }
}

pub fn unescape_octal(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let d1 = bytes[i + 1];
            let d2 = bytes[i + 2];
            let d3 = bytes[i + 3];
            if (b'0'..=b'7').contains(&d1)
                && (b'0'..=b'7').contains(&d2)
                && (b'0'..=b'7').contains(&d3)
            {
                let val = ((d1 - b'0') << 6) | ((d2 - b'0') << 3) | (d3 - b'0');
                out.push(val);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn fmt_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (idx, ch) in s.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt_bytes() {
        assert_eq!(fmt_bytes(None, 1), "—");
        assert_eq!(fmt_bytes(Some(f64::NAN), 1), "—");
        assert_eq!(fmt_bytes(Some(f64::INFINITY), 1), "—");
        assert_eq!(fmt_bytes(Some(0.0), 1), "0 B");
        assert_eq!(fmt_bytes(Some(512.0), 1), "512 B");
        assert_eq!(fmt_bytes(Some(1000.0), 1), "1.0 KB");
        assert_eq!(fmt_bytes(Some(10.0 * 1000.0 * 1000.0), 1), "10.0 MB");
        assert_eq!(fmt_bytes(Some(6_082_924_544.0), 1), "6.1 GB");
        assert_eq!(fmt_bytes(Some(2.5 * 1000.0_f64.powi(3)), 1), "2.5 GB");
        assert_eq!(fmt_bytes(Some(1000.0_f64.powi(4)), 1), "1.0 TB");
    }

    #[test]
    fn test_fmt_rate() {
        assert_eq!(fmt_rate(None), "—");
        assert_eq!(fmt_rate(Some(1000.0)), "1.0 KB/s");
    }

    #[test]
    fn test_fmt_pct() {
        assert_eq!(fmt_pct(None), "—");
        assert_eq!(fmt_pct(Some(0.0)), "0%");
        assert_eq!(fmt_pct(Some(4.24)), "4.2%");
        assert_eq!(fmt_pct(Some(75.8)), "76%");
    }

    #[test]
    fn test_fmt_uptime_and_cpu_time() {
        assert_eq!(fmt_uptime(0.0), "0:00:00");
        assert_eq!(fmt_uptime(65.0), "0:01:05");
        assert_eq!(fmt_uptime(3665.0), "1:01:05");
        assert_eq!(fmt_uptime(90065.0), "1d 1:01:05");
        assert_eq!(fmt_cpu_time(0.0), "0:00:00");
        assert_eq!(fmt_cpu_time(72.0), "0:01:12");
        assert_eq!(fmt_cpu_time(3600.0), "1:00:00");
    }

    #[test]
    fn test_fmt_clock() {
        assert_eq!(fmt_clock(0.0), "—");
        let s = fmt_clock(1700000000.0);
        assert_eq!(s.len(), 5);
        assert_eq!(&s[2..3], ":");
    }

    #[test]
    fn test_nice_label_and_max() {
        assert_eq!(nice_label(-20), "High");
        assert_eq!(nice_label(-10), "High");
        assert_eq!(nice_label(-5), "Above normal");
        assert_eq!(nice_label(0), "Normal");
        assert_eq!(nice_label(5), "Below normal");
        assert_eq!(nice_label(15), "Low");

        assert_eq!(nice_max(0.0), 1.0);
        assert_eq!(nice_max(-10.0), 1.0);
        assert_eq!(nice_max(8.0), 10.0);
        assert_eq!(nice_max(15.0), 20.0);
        assert_eq!(nice_max(42.0), 50.0);
        assert_eq!(nice_max(88.0), 100.0);
    }

    #[test]
    fn test_unescape_octal() {
        assert_eq!(
            unescape_octal(r"/media/user/USB\040Drive"),
            "/media/user/USB Drive"
        );
        assert_eq!(unescape_octal("/mnt/data"), "/mnt/data");
    }

    #[test]
    fn test_freq_to_channel() {
        assert_eq!(freq_to_channel(2.412), Some(1));
        assert_eq!(freq_to_channel(2.484), Some(14));
        assert_eq!(freq_to_channel(5.180), Some(36));
        assert_eq!(freq_to_channel(0.0), None);
    }
}

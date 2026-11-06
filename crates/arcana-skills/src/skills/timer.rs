//! 타이머: "라면 3분 타이머", "1시간 30분 뒤에 알려줘"

use crate::{Context, Event, Reply, Skill};
use arcana_core::Understanding;
use regex::Regex;
use std::sync::OnceLock;

pub struct Timer;

fn kor_num(s: &str) -> Option<u64> {
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    let digit = |w: &str| -> Option<u64> {
        Some(match w {
            "일" | "한" => 1,
            "이" | "두" => 2,
            "삼" | "세" => 3,
            "사" | "네" => 4,
            "오" | "다섯" => 5,
            "육" | "여섯" => 6,
            "칠" | "일곱" => 7,
            "팔" | "여덟" => 8,
            "구" | "아홉" => 9,
            "열" => 10,
            _ => return None,
        })
    };
    if let Some(n) = digit(s) {
        return Some(n);
    }
    // 이십오 → 25
    let (a, b) = s.split_once('십')?;
    let tens = if a.is_empty() { 1 } else { digit(a)? };
    let ones = if b.is_empty() { 0 } else { digit(b)? };
    Some(tens * 10 + ones)
}

pub fn parse_duration(text: &str) -> Option<u64> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"([0-9]+|[일이삼사오육칠팔구십한두세네다섯여섯일곱여덟아홉열]+)\s*(시간|분|초)")
            .unwrap()
    });
    let mut sec = 0;
    for c in re.captures_iter(text) {
        let n = kor_num(&c[1])?;
        sec += n * match &c[2] {
            "시간" => 3600,
            "분" => 60,
            _ => 1,
        };
    }
    (sec > 0).then_some(sec)
}

pub fn fmt_duration(sec: u64) -> String {
    let (h, m, s) = (sec / 3600, sec % 3600 / 60, sec % 60);
    [(h, "시간"), (m, "분"), (s, "초")]
        .iter()
        .filter(|(v, _)| *v > 0)
        .map(|(v, u)| format!("{v}{u}"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Skill for Timer {
    fn intents(&self) -> &[&'static str] {
        &["set_timer"]
    }

    fn describe(&self) -> &'static str {
        "타이머 — \"라면 3분 타이머\", \"10분 있다 불러줘\""
    }

    fn run(&self, _: &str, u: &Understanding, cx: &Context) -> Reply {
        let sec = u
            .slots
            .get("TIME")
            .and_then(|t| parse_duration(t))
            .or_else(|| parse_duration(&cx.text));
        let Some(sec) = sec else {
            return Reply::error("몇 분 뒤인지 못 알아들었어. '5분 타이머'처럼 말해줘.");
        };
        let label = fmt_duration(sec);
        let events = cx.events.clone();
        let l = label.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(sec));
            events(Event::Alert {
                title: "타이머".into(),
                body: format!("{l} 지났어!"),
            });
        });
        Reply::say(format!("{label} 뒤에 알려줄게."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations() {
        assert_eq!(parse_duration("3분"), Some(180));
        assert_eq!(parse_duration("1시간 30분"), Some(5400));
        assert_eq!(parse_duration("이십오분"), Some(1500));
        assert_eq!(parse_duration("두시간"), Some(7200));
        assert_eq!(parse_duration("라면"), None);
        assert_eq!(fmt_duration(5430), "1시간 30분 30초");
    }
}

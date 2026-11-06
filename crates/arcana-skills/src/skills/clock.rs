use crate::{Context, Reply, Skill};
use arcana_core::Understanding;
use chrono::{Datelike, Local, Timelike};

pub struct Clock;

const WEEK: [&str; 7] = ["월", "화", "수", "목", "금", "토", "일"];

impl Skill for Clock {
    fn intents(&self) -> &[&'static str] {
        &["ask_time", "ask_date"]
    }

    fn describe(&self) -> &'static str {
        "시간·날짜 — \"지금 몇 시야\", \"오늘 며칠이야\""
    }

    fn run(&self, intent: &str, _: &Understanding, _: &Context) -> Reply {
        let n = Local::now();
        if intent == "ask_time" {
            let ampm = if n.hour() < 12 { "오전" } else { "오후" };
            let h = if n.hour() % 12 == 0 {
                12
            } else {
                n.hour() % 12
            };
            Reply::say(format!("지금 {ampm} {h}시 {}분이야.", n.minute()))
        } else {
            Reply::say(format!(
                "오늘은 {}년 {}월 {}일 {}요일이야.",
                n.year(),
                n.month(),
                n.day(),
                WEEK[n.weekday().num_days_from_monday() as usize]
            ))
        }
    }
}

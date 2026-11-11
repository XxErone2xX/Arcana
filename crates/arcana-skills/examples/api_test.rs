//! 외부 AI API 시험: cargo run --example api_test -- <provider> <base> <model> [key]
use arcana_skills::{config::Config, context::Turn, external};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut c = Config::default();
    c.api_provider = a[1].clone();
    c.api_base = a[2].clone();
    c.api_model = a[3].clone();
    c.api_key = a.get(4).cloned().unwrap_or_default();
    let h = vec![Turn { time: "-".into(), user: "안녕".into(), reply: "안녕하세요.".into(), intent: "greet".into(), slots: Default::default() }];
    println!("{} → {:?}", external::label(&c), external::ask(&c, &h, "오늘 뭐 할까?", Some("얼로니")));
}

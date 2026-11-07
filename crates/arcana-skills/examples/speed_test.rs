//! 탐색 속도 시험: cargo run --example speed_test -- <시작 주소> <일꾼 수> <초>
use arcana_skills::{config::Config, knowledge::Knowledge};
use std::sync::{Arc, RwLock};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::env::temp_dir().join("arcana-speed");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("seeded_world"), "1").unwrap();
    std::fs::write(dir.join("frontier.json"), format!("{{\"queue\":[\"{}\"]}}", a[1])).unwrap();
    let k = Arc::new(Knowledge::open(dir.clone()));
    let mut c = Config::default();
    c.self_study = true;
    c.study_workers = a[2].parse().unwrap();
    c.wiki_langs = vec![];
    let cfg = Arc::new(RwLock::new(c));
    k.start_explorer(cfg, Arc::new(|_| {}));
    std::thread::sleep(std::time::Duration::from_secs(a[3].parse().unwrap()));
    println!("일꾼 {}명: {}초 동안 {}개", a[2], a[3], k.stats().count);
}

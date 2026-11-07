//! 실제 인터넷 연결 확인:  cargo run -p arcana-skills --example net_check
use arcana_skills::knowledge::{CrawlSettings, Knowledge, Learned, Studied};

fn main() {
    println!("── /Diag ──");
    for l in arcana_skills::net::diagnose() {
        println!("{l}");
    }
    let dir = std::env::temp_dir().join("arcana-net-check");
    let _ = std::fs::remove_dir_all(&dir);
    let k = Knowledge::open(dir.clone());

    println!("\n── 지식 질문: 블랙홀 ──");
    match k.learn("블랙홀", "질문") {
        Ok(Learned::Doc(d)) => println!("{} — {}\n출처: {}", d.title, d.summary, d.url),
        Ok(Learned::Ambiguous { title, options }) => println!("여러 뜻: {title} {options:?}"),
        Ok(Learned::NotFound) => println!("못 찾음"),
        Err(e) => println!("오류: {e}"),
    }
    println!("\n── 검색: 러스트 프로그래밍 언어 ──");
    match k.search("러스트 프로그래밍 언어", "") {
        Ok(h) => h
            .iter()
            .for_each(|h| println!("- {} | {} | {}", h.title, h.source, h.url)),
        Err(e) => println!("오류: {e}"),
    }
    println!("\n── 페이지 읽기 ──");
    match k.read_page("https://www.rust-lang.org/", &[]) {
        Ok(p) => println!("{} — {}\n출처: {}", p.title, p.summary, p.url),
        Err(e) => println!("오류: {e}"),
    }
    println!("\n── 스스로 학습 3걸음 ──");
    let s = CrawlSettings {
        wiki_langs: vec!["ko".into(), "en".into()],
        explore_web: true,
        blocked_domains: vec![],
        site_delay_secs: 0,
        site_daily_limit: 100,
        obey_robots: true,
        only: None,
    };
    for _ in 0..3 {
        match k.study_step(&s) {
            Ok(Studied::Learned { title, site }) => println!("배움: {title} ({site})"),
            Ok(Studied::Wait) => println!("대기"),
            Ok(Studied::Idle) => println!("없음"),
            Err(e) => println!("오류: {e}"),
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

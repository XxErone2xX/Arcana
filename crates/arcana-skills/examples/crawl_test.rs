//! 웹 탐색 테스트:  cargo run -p arcana-skills --example crawl_test -- <시작 주소>
use arcana_skills::knowledge::{CrawlSettings, Knowledge, Studied};

fn main() {
    let start = std::env::args().nth(1).expect("시작 주소");
    let dir = std::env::temp_dir().join("arcana-crawl-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("seeded_world"), "1").unwrap();
    std::fs::write(
        dir.join("frontier.json"),
        format!("{{\"queue\":[\"{start}\"]}}"),
    )
    .unwrap();
    let k = Knowledge::open(dir.clone());
    let s = CrawlSettings {
        wiki_langs: vec![],
        explore_web: true,
        blocked_domains: vec![],
        site_delay_secs: 3,
        site_daily_limit: 100,
        only: std::env::args().nth(2), // 둘째 인자: 이 사이트에서만
    };
    for i in 0..12 {
        match k.study_step(&s) {
            Ok(Studied::Learned { title, site }) => println!("[{i}] 배움: {title}  ({site})"),
            Ok(Studied::Wait) => {
                println!("[{i}] 사이트 간격 대기");
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
            Ok(Studied::Idle) => {
                println!("[{i}] 더 읽을 곳 없음");
                break;
            }
            Err(e) => {
                println!("[{i}] 오류: {e}");
                break;
            }
        }
    }
    println!("공책: {:?}", k.stats().recent);
    println!("\n── 채팅에서 페이지 읽기 ──");
    match k.read_page(&start, &[]) {
        Ok(p) => println!(
            "제목: {}\n요약: {}\n소제목: {:?}\n링크: {:?}\n출처: {} — {}",
            p.title, p.summary, p.headings, p.links, p.site, p.url
        ),
        Err(e) => println!("오류: {e}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

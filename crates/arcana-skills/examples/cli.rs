//! 터미널에서 테스트:  cargo run -p arcana-skills --example cli
use arcana_skills::Assistant;
use std::io::{BufRead, Write};
use std::sync::Arc;

fn main() {
    let brain = arcana_core::builtin::load().expect("모델 로드 실패");
    let dir = std::env::temp_dir().join("arcana-cli");
    let talker = arcana_core::builtin::load_chat()
        .ok()
        .map(|t| Arc::new(t) as Arc<dyn arcana_core::Talker>);
    let a = Assistant::new(
        Box::new(brain),
        talker,
        dir,
        Arc::new(|e| println!("\n[알림] {e:?}")),
    );
    println!(
        "{} / {:?} — 빠진 스킬: {:?}",
        a.brain_name(),
        a.talker_name(),
        a.missing_skills()
    );
    let stdin = std::io::stdin();
    loop {
        print!("> ");
        std::io::stdout().flush().unwrap();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap() == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // "학습 " 으로 시작하면 학습 탭 입력처럼 처리 (예: "학습 /Start")
        if let Some(cmd) = line.strip_prefix("학습 ") {
            let r = a.study(cmd);
            for l in &r.lines {
                println!("[학습] {l}");
            }
            if let Some(q) = &r.question {
                println!("[학습 질문] {q}");
            }
            println!("[학습 상태] running={} {:?}", r.running, r.stats);
            continue;
        }
        let r = a.handle(line);
        println!(
            "   [{} {:.0}% {:?}]",
            r.understanding.intent,
            r.understanding.confidence * 100.0,
            r.understanding.slots
        );
        println!("{}", r.reply.text);
        if let Some(q) = &r.reply.confirm {
            println!("   (확인 필요: {q}) → 테스트에선 자동 '아니오'");
        }
    }
}

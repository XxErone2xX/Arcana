//! 터미널에서 테스트:  cargo run -p arcana-skills --example cli
use arcana_skills::Assistant;
use std::io::{BufRead, Write};
use std::sync::Arc;

fn main() {
    let brain = arcana_core::builtin::load().expect("모델 로드 실패");
    let dir = std::env::temp_dir().join("arcana-cli");
    let a = Assistant::new(
        Box::new(brain),
        dir,
        Arc::new(|e| println!("\n[알림] {e:?}")),
    );
    println!("{} — 빠진 스킬: {:?}", a.brain_name(), a.missing_skills());
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

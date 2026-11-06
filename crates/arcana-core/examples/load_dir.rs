//! 폴더의 대화 모델 테스트:  cargo run -p arcana-core --example load_dir -- <models 폴더>
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("폴더 경로"));
    let w = std::fs::read(dir.join("arcana-chat.safetensors")).unwrap();
    let j = std::fs::read_to_string(dir.join("arcana-chat.json")).unwrap();
    let t = arcana_core::ArcanaChat::from_bytes(&w, &j).expect("로드");
    let mut rng = arcana_core::gpt::Rng::new(1);
    for s in ["새 키보드 샀어", "새 모니터 샀어", "심심해", "뷁", "피곤해"] {
        println!(
            "{:.2}  {s} → {}",
            t.familiarity(s),
            t.generate(s, 0.0, &mut rng)
        );
    }
}

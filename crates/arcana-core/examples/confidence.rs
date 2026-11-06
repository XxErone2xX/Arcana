//! 대화 모델 자신감 확인:  cargo run -p arcana-core --example confidence
fn main() {
    let t = arcana_core::builtin::load_chat().unwrap();
    let mut rng = arcana_core::gpt::Rng::new(7);
    for s in [
        "배고파",
        "오늘 너무 피곤해",
        "롤 졌어",
        "고마워",
        "콜라 먹고 싶다",
        "심심해",
        "기분 좋아",
        "흐아앙ㅇ랄",
        "양자역학의 불확정성 원리",
        "어제 본 영화 줄거리 말해줘",
        "주식 사야 될까",
        "qwerasdf",
        "ㅁㄴㅇㄹ",
    ] {
        let (r, _) = t.generate_scored(s, 0.7, &mut rng);
        println!("{:.2}  {s}  →  {r}", t.familiarity(s));
    }
}

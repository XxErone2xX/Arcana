//! Rust 엔진이 PyTorch랑 똑같은 답을 내는지 검사
//! 모델을 다시 학습하면 training/export.py 가 golden.json도 새로 만들어 줌.

use arcana_core::{builtin, tensor::argmax};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    text: String,
    intent_probs: Vec<f32>,
    slot_tags: Vec<usize>,
}

#[test]
fn matches_pytorch() {
    let brain = builtin::load().expect("모델 로드");
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../../models/golden.json")).unwrap();
    for c in cases {
        let out = brain.raw(&c.text);
        let max_diff = out
            .intent_probs
            .iter()
            .zip(&c.intent_probs)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(max_diff < 1e-3, "{}: 확률 차이 {max_diff}", c.text);
        for (i, &t) in c.slot_tags.iter().enumerate() {
            assert_eq!(
                argmax(out.slot_logits.row(i)),
                t,
                "{}: {}번째 글자 태그 다름",
                c.text,
                i
            );
        }
    }
}

#[derive(Deserialize)]
struct ChatCase {
    text: String,
    greedy: String,
}

/// 대화 모델 계산 검사: 학습 안 한 작은 무작위 모델로 Rust = PyTorch 확인 (training/make_test_fixture.py)
#[test]
fn chat_matches_pytorch() {
    let talker = arcana_core::ArcanaChat::from_bytes(
        include_bytes!("fixtures/chat-test.safetensors"),
        include_str!("fixtures/chat-test.json"),
    )
    .expect("테스트 모델 로드");
    let cases: Vec<ChatCase> =
        serde_json::from_str(include_str!("fixtures/golden-chat.json")).unwrap();
    let mut rng = arcana_core::gpt::Rng::new(1);
    for c in &cases {
        let got = talker.generate(&c.text, 0.0, &mut rng);
        assert!(
            got.starts_with(&c.greedy),
            "{}: rust '{}' / torch '{}'",
            c.text,
            got,
            c.greedy
        );
    }
}

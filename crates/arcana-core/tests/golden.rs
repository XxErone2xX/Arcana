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

#[test]
fn chat_matches_pytorch() {
    let talker = builtin::load_chat().expect("대화 모델 로드");
    let cases: Vec<ChatCase> =
        serde_json::from_str(include_str!("../../../models/golden-chat.json")).unwrap();
    let mut rng = arcana_core::gpt::Rng::new(1);
    let mut same = 0;
    for c in &cases {
        let got = talker.generate(&c.text, 0.0, &mut rng);
        if got == c.greedy {
            same += 1;
        } else {
            eprintln!("다름: {} → rust '{}' / torch '{}'", c.text, got, c.greedy);
        }
    }
    // 부동소수점 미세 차이로 가끔 1등 글자가 바뀔 수 있어서 거의 다 맞으면 통과
    assert!(
        same * 10 >= cases.len() * 9,
        "{}/{} 일치",
        same,
        cases.len()
    );
}

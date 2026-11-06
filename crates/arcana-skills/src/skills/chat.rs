//! 대화: 명령이 아닌 말은 Arcana 대화 모델이 직접 대답을 만들어냄.
//! (대화 모델이 없으면 정해진 문장으로 대답)

use crate::knowledge::Question;
use crate::{Context, Reply, Skill};

/// 대화 모델 자신감이 이보다 낮으면 "처음 보는 말"
const UNFAMILIAR: f32 = 0.5;
use arcana_core::Understanding;

pub struct Chat;

/// 검색해 볼 만한 문장인지 ("ㅁㄴㅇㄹ", "qwerasdf" 같은 건 안 찾음)
fn looks_meaningful(text: &str) -> bool {
    let syllables = text.chars().filter(|c| ('가'..='힣').contains(c)).count();
    let words: Vec<&str> = text.split_whitespace().collect();
    let latin_words = words
        .iter()
        .filter(|w| {
            w.chars().all(|c| c.is_ascii_alphabetic())
                && w.len() >= 2
                && w.chars().any(|c| "aeiouAEIOU".contains(c))
        })
        .count();
    syllables >= 3 || latin_words >= 2
}

fn pick<'a>(opts: &[&'a str]) -> &'a str {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    opts[n as usize % opts.len()]
}

impl Skill for Chat {
    fn intents(&self) -> &[&'static str] {
        &["greet", "thanks", "bye", "who_are_you", "unknown"]
    }

    fn describe(&self) -> &'static str {
        "대화 — \"배고파\", \"오늘 너무 피곤해\", \"재밌는 얘기 해줘\""
    }

    fn run(&self, intent: &str, _: &Understanding, cx: &Context) -> Reply {
        let name = cx.cfg().name.clone();
        match intent {
            "bye" => Reply {
                text: "네, 필요하시면 언제든 다시 불러 주십시오.".into(),
                quit: true,
                ..Default::default()
            },
            "who_are_you" => Reply::say(format!(
                "저는 {name}입니다. 이 PC에서 일하는 비서입니다.\n왼쪽 위 메뉴(☰)를 누르시면 제가 할 수 있는 일을 보실 수 있습니다."
            )),
            _ => {
                // 인사, 감사, 잡담 → 대화 모델이 직접 대답
                if let Some(t) = &cx.talker {
                    if t.familiarity(&cx.text) < UNFAMILIAR {
                        // 처음 보는 말 → 지어내지 않고, 학습 탭에서 어떻게 대답할지 물어봄
                        if let Some(k) = &cx.knowledge {
                            let q = Question::Reply { phrase: cx.text.trim().to_string() };
                            if k.ask(q.clone()) {
                                cx.emit(crate::Event::Question { text: q.text() });
                            }
                        }
                        // 인터넷에서 찾아보기 (말이 되는 문장일 때만)
                        if looks_meaningful(&cx.text) {
                            if let Ok(Some(found)) = crate::skills::web::search_text(cx.text.trim(), cx) {
                                return Reply::say(format!(
                                    "제가 아직 잘 모르는 말이라 인터넷에서 찾아봤습니다.\n\n{found}\n\n(학습 탭에서 어떻게 대답하면 좋을지 알려 주시면 다음부터 그렇게 대답하겠습니다.)"
                                ));
                            }
                        }
                        return Reply::say(pick(&[
                            "죄송합니다. 아직 잘 모르는 말입니다. 학습 탭에서 어떻게 대답하면 좋을지 알려 주시면 배우겠습니다.",
                            "그 말은 아직 배우지 못했습니다. 학습 탭에 질문을 남겨 두었으니 알려 주시면 기억하겠습니다.",
                        ]));
                    }
                    // 생각하기: 후보를 여러 개 만들고 (피드백·자연스러움·다양성·길이) 점수로 제일 좋은 걸 고름
                    let (depth, temp, top_k, long) = {
                        let c = cx.cfg();
                        (c.think_depth.clamp(1, 32), c.chat_temperature.clamp(0.1, 2.0), c.chat_top_k.clamp(1, 64), c.prefer_long_replies)
                    };
                    let cands = t.candidates_scored(&cx.text, depth, temp, top_k);
                    let best = match &cx.knowledge {
                        Some(k) => k.choose(&cx.text, cands, long),
                        None => cands.into_iter().map(|(c, _)| c).find(|c| !c.trim().is_empty()),
                    };
                    if let Some(r) = best {
                        return Reply::chat(r.trim());
                    }
                }
                Reply::say(match intent {
                    "greet" => pick(&["안녕하세요. 무엇을 도와드릴까요?", "반갑습니다. 필요한 게 있으시면 말씀해 주십시오."]),
                    "thanks" => pick(&["별말씀을요. 언제든 불러 주십시오.", "도움이 됐다니 다행입니다."]),
                    _ => pick(&["죄송합니다. 그건 아직 배우지 못했습니다.", "잘 이해하지 못했습니다. 다르게 말씀해 주시겠습니까?"]),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn meaningful() {
        assert!(super::looks_meaningful("주식 사야 될까"));
        assert!(super::looks_meaningful("what is rust language"));
        assert!(!super::looks_meaningful("ㅁㄴㅇㄹ"));
        assert!(!super::looks_meaningful("qwerasdf"));
        assert!(!super::looks_meaningful("흐ㅇ"));
    }
}

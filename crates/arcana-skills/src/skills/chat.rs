//! 대화: 명령이 아닌 말은 Arcana 대화 모델이 직접 대답을 만들어냄.
//! (대화 모델이 없으면 정해진 문장으로 대답)

use crate::knowledge::Question;
use crate::{Context, Reply, Skill};

/// 대화 모델 자신감이 이보다 낮으면 "처음 보는 말"
const UNFAMILIAR: f32 = 0.5;
/// 대답 후보의 자연스러움(고른 글자들의 평균 확률)이 이보다 낮으면 문장이 깨진 것으로 보고 버림
const MIN_FLUENCY: f32 = 0.85;
use arcana_core::Understanding;

pub struct Chat;

/// 모르는 말: 지어내지 않고 → 학습 탭에 질문 남기고 → 인터넷에서 찾아봄
fn unfamiliar(cx: &Context) -> Reply {
    if let Some(k) = &cx.knowledge {
        let q = Question::Reply {
            phrase: cx.text.trim().to_string(),
        };
        if k.ask(q.clone()) {
            cx.emit(crate::Event::Question { text: q.text() });
        }
    }
    if looks_meaningful(&cx.text) {
        if let Ok(Some(found)) = crate::skills::web::search_text(cx.text.trim(), cx) {
            return Reply::say(format!(
                "제가 아직 잘 모르는 말이라 인터넷에서 찾아봤습니다.\n\n{found}\n\n(학습 탭에서 어떻게 대답하면 좋을지 알려 주시면 다음부터 그렇게 대답하겠습니다.)"
            ));
        }
    }
    Reply::say(pick(&[
        "죄송합니다. 아직 잘 모르는 말입니다. 학습 탭에서 어떻게 대답하면 좋을지 알려 주시면 배우겠습니다.",
        "그 말은 아직 배우지 못했습니다. 학습 탭에 질문을 남겨 두었으니 알려 주시면 기억하겠습니다.",
    ]))
}

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

/// PrimeThink 가 "이 대답이면 완벽하다"고 인정하는 기준
const PERFECT_FLUENCY: f32 = 0.96;
const PERFECT_MAX_REPEAT: f32 = 0.35;
/// PrimeThink 최대 생각 시간
const PRIME_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// 생각하기: 후보를 여러 개 만들고 채점해서 제일 좋은 걸 고름.
/// 생각 수치가 높을수록 후보가 많고, 여러 번 다시 생각함 (매번 조금 더 신중하게 = 온도를 낮춤).
/// PrimeThink 는 스스로 채점해서 완벽 기준(자연스러움·안 겹침·나쁜 피드백 없음)을 넘을 때까지 계속 생각함.
fn deliberate(
    t: &dyn arcana_core::Talker,
    cx: &Context,
    think: &crate::config::Think,
    temp: f32,
    top_k: usize,
    long: bool,
) -> Option<String> {
    let start = std::time::Instant::now();
    let mut best: Option<(f32, String)> = None;
    for round in 0..think.rounds {
        let temp = (temp * (1.0 - 0.06 * round as f32)).max(0.3);
        let n = think.candidates + round * 4;
        let cands = t.candidates_scored(&cx.text, n, temp, top_k);
        let mut perfect = false;
        for (c, fluency) in cands {
            if fluency < MIN_FLUENCY || c.trim().chars().count() < 2 {
                continue; // 문장이 깨진 후보는 버림
            }
            let (score, repeat, reward) = match &cx.knowledge {
                Some(k) => {
                    let (s, r) = k.judge(&cx.text, &c, fluency, long);
                    (s, r, k.reward(&cx.text, &c))
                }
                None => (fluency, 0.0, 0.0),
            };
            if think.prime && fluency >= PERFECT_FLUENCY && repeat <= PERFECT_MAX_REPEAT && reward >= 0.0 {
                perfect = true;
            }
            if best.as_ref().map_or(true, |(b, _)| score > *b) {
                best = Some((score, c));
            }
        }
        if think.prime {
            cx.emit(crate::Event::Thinking {
                text: if perfect { format!("PrimeThink · {}번째 생각에서 완벽한 대답을 찾았습니다", round + 1) } else { format!("PrimeThink · {}번째 생각 중…", round + 1) },
            });
            if perfect || start.elapsed() > PRIME_BUDGET {
                break;
            }
        }
    }
    let pick = best.map(|(_, c)| c)?;
    if let Some(k) = &cx.knowledge {
        k.remember_pick(&pick);
    }
    Some(pick)
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
            // 인사·감사는 정해진 공손한 대답이 더 안정적 (가르쳐 준 대답이 있으면 그게 먼저 나감)
            "greet" => Reply::chat(match &cx.user_name {
                Some(n) => pick(&["안녕하세요, {n}님. 무엇을 도와드릴까요?", "반갑습니다, {n}님. 필요한 게 있으시면 말씀해 주십시오.", "{n}님, 안녕하세요! 오늘 하루는 어떠셨습니까?"]).replace("{n}", n),
                None => pick(&["안녕하세요. 무엇을 도와드릴까요?", "반갑습니다. 필요한 게 있으시면 말씀해 주십시오.", "안녕하세요! 오늘 하루는 어떠셨습니까?"]).to_string(),
            }),
            "thanks" => Reply::chat(pick(&["별말씀을요. 언제든 불러 주십시오.", "도움이 됐다니 다행입니다.", "천만의 말씀입니다. 또 필요한 게 있으시면 말씀해 주십시오."])),
            _ => {
                // 잡담 → 대화 모델이 직접 대답
                if let Some(t) = &cx.talker {
                    if t.familiarity(&cx.text) < UNFAMILIAR {
                        return unfamiliar(cx);
                    }
                    let (think, temp, top_k, long) = {
                        let c = cx.cfg();
                        (c.think(), c.chat_temperature.clamp(0.1, 2.0), c.chat_top_k.clamp(1, 64), c.prefer_long_replies)
                    };
                    if let Some(r) = deliberate(t.as_ref(), cx, &think, temp, top_k, long) {
                        return Reply::chat(r.trim());
                    }
                }
                // 대화 모델이 없거나, 자연스럽게 대답할 자신이 없음 → 지어내지 않고 검색·질문
                unfamiliar(cx)
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

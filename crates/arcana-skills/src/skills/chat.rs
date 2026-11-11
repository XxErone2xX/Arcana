//! 대화: 명령이 아닌 말은 Arcana 대화 모델이 직접 대답을 만들어냄.
//! (대화 모델이 없으면 정해진 문장으로 대답)

use crate::knowledge::Question;
use crate::{Context, Reply, Skill};

// 기준값(처음 보는 말·깨진 문장)은 설정 파일의 chat_min_familiarity / chat_min_fluency
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

/// 외부 AI에게 대답을 받음. 실패하면 이유를 알리고 원래 방식(검색·질문)으로
/// (외부 AI 대답은 Arcana 학습 재료로 안 씀 → from_chat 아님, 👍/👎 없음)
fn external(cx: &Context) -> Reply {
    let c = cx.cfg().clone();
    cx.emit(crate::Event::Thinking { text: format!("{}에게 묻는 중…", crate::external::label(&c)) });
    match crate::external::ask(&c, &cx.recent, cx.text.trim(), cx.user_name.as_deref()) {
        Ok(t) => Reply { text: t, via: Some(crate::external::label(&c)), ..Default::default() },
        Err(e) => {
            let mut r = unfamiliar(cx);
            r.text = format!("외부 AI에 연결하지 못했습니다. {e}\n\n{}", r.text);
            r
        }
    }
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

/// 대답 다듬기: 완성된 문장까지만 (중간에 끊긴 꼬리 제거), 너무 길면 앞 2~3문장만
fn tidy_reply(c: &str) -> String {
    let c = c.trim();
    let mut out = String::new();
    let mut n = 0;
    let mut start = 0;
    for (i, ch) in c.char_indices() {
        if matches!(ch, '.' | '?' | '!') {
            let end = i + ch.len_utf8();
            out.push_str(&c[start..end]);
            start = end;
            n += 1;
            if n >= 3 || out.chars().count() >= 140 {
                break;
            }
        }
    }
    if out.trim().is_empty() {
        c.to_string() // 문장 부호가 없는 짧은 대답은 그대로
    } else {
        out.trim().to_string()
    }
}

/// 조사로 시작하는 대답 ("에 속하지 않도록…") = 앞부분이 잘린 문장
fn broken_start(c: &str) -> bool {
    let first = c.split_whitespace().next().unwrap_or("");
    matches!(first, "에" | "를" | "을" | "은" | "는" | "이" | "가" | "의" | "로" | "와" | "과" | "도" | "에서" | "으로" | "하고")
}

/// 같은 낱말을 세 번 이상 되풀이하면 깨진 대답 ("Chrome을 연 다음 Chrome을 연 다음 Chrome…")
fn repetitive(c: &str) -> bool {
    let words: Vec<&str> = c.split_whitespace().filter(|w| w.chars().count() >= 2).collect();
    let mut counts = std::collections::HashMap::new();
    for w in &words {
        *counts.entry(*w).or_insert(0) += 1;
    }
    counts.values().any(|&n| n >= 3)
}

/// PrimeThink 가 "이 대답이면 완벽하다"고 인정하는 기준
const PERFECT_FLUENCY: f32 = 0.96;
const PERFECT_MAX_REPEAT: f32 = 0.35;
/// PrimeThink: 시간 제한 없음. 단, 이만큼 연속으로 더 나은 대답을 못 찾으면 지금까지 중 최선으로 대답
/// (모델 실력상 '완벽' 기준에 도달할 수 없는 말에서 영원히 멈춰 있지 않게)
const PRIME_PATIENCE: usize = 40;

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
    let min_fluency = cx.cfg().chat_min_fluency;
    let mut best: Option<(f32, String)> = None;
    let mut stale = 0usize;
    let rounds = if think.prime { usize::MAX } else { think.rounds };
    for round in 0..rounds {
        let before = best.as_ref().map(|(b, _)| *b);
        let temp = (temp * (1.0 - 0.06 * round.min(10) as f32)).max(0.3);
        let n = think.candidates + round.min(8) * 4;
        let cands = t.candidates_scored(&cx.text, n, temp, top_k);
        let mut perfect = false;
        for (c, fluency) in cands {
            let c = tidy_reply(&c);
            if fluency < min_fluency || c.chars().count() < 2 || repetitive(&c) || broken_start(&c) {
                continue; // 문장이 깨졌거나 같은 말을 되풀이하는 후보는 버림
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
            let improved = best.as_ref().map(|(b, _)| *b) != before;
            stale = if improved { 0 } else { stale + 1 };
            if perfect || stale >= PRIME_PATIENCE {
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
                let (ext, always) = {
                    let c = cx.cfg();
                    (crate::external::enabled(&c), c.api_mode == "always")
                };
                // 외부 AI 설정에서 "대화는 전부 외부 AI"
                if ext && always {
                    return external(cx);
                }
                // 잡담 → 대화 모델이 직접 대답
                if let Some(t) = &cx.talker {
                    if t.familiarity(&cx.text) < cx.cfg().chat_min_familiarity {
                        return if ext { external(cx) } else { unfamiliar(cx) };
                    }
                    let (think, temp, top_k, long) = {
                        let c = cx.cfg();
                        (c.think(), c.chat_temperature.clamp(0.1, 2.0), c.chat_top_k.clamp(1, 64), c.prefer_long_replies)
                    };
                    if let Some(r) = deliberate(t.as_ref(), cx, &think, temp, top_k, long) {
                        return Reply::chat(r.trim());
                    }
                }
                // 대화 모델이 없거나, 자연스럽게 대답할 자신이 없음 → 외부 AI(설정했으면) 또는 검색·질문
                if ext { external(cx) } else { unfamiliar(cx) }
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

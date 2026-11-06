//! 지식 질문: "블랙홀이 뭐야", "세종대왕은 누구야"
//! 공책에 있으면 바로, 없으면 위키백과에서 찾아서 저장하고 대답.
//! 위키백과에도 없으면 학습 탭에서 사용자에게 물어봄.

use crate::knowledge::{Doc, Learned, Question};
use crate::util::eun;
use crate::{Context, Reply, Skill};
use arcana_core::Understanding;

pub struct Knowledge;

const JOSA: &[&str] = &[
    "에 대해",
    "이란",
    "란",
    "이",
    "가",
    "은",
    "는",
    "을",
    "를",
    "에",
    "의",
    "도",
];

/// 모델이 긴 단어를 중간에서 자르는 경우 보정: "양자컴" + 원문 "양자컴퓨터가" → "양자컴퓨터"
pub fn complete_topic(slot: &str, text: &str) -> String {
    let slot = slot.trim();
    let Some(word) = text.split_whitespace().find(|w| w.starts_with(slot)) else {
        return slot.to_string();
    };
    let rest = &word[slot.len()..];
    let rest = rest.trim_end_matches(|c: char| !c.is_alphanumeric());
    if rest.is_empty() || JOSA.contains(&rest) {
        return slot.to_string();
    }
    let mut full = format!("{slot}{rest}");
    for j in JOSA {
        if let Some(stripped) = full.strip_suffix(j) {
            if stripped.chars().count() >= slot.chars().count() {
                full = stripped.to_string();
                break;
            }
        }
    }
    full
}

fn answer(doc: &Doc, fresh: bool) -> String {
    let head = if fresh {
        format!("{}에 대해 찾아봤습니다.", doc.title)
    } else {
        format!(
            "{}{} 제가 알고 있는 내용입니다.",
            doc.title,
            eun(&doc.title)
        )
    };
    let source = if doc.url.is_empty() {
        "출처: 직접 가르쳐 주신 내용".to_string()
    } else {
        format!("출처: 위키백과 — {}", doc.url)
    };
    let tail = if fresh {
        "\n(공책에 저장해 뒀습니다. 다음에는 바로 알려 드리겠습니다.)"
    } else {
        ""
    };
    format!("{head}\n\n{}\n\n{source}{tail}", doc.summary)
}

impl Skill for Knowledge {
    fn intents(&self) -> &[&'static str] {
        &["ask_knowledge"]
    }

    fn describe(&self) -> &'static str {
        "지식 — \"블랙홀이 뭐야\", \"세종대왕은 누구야\" (모르면 찾아보고 기억함)"
    }

    fn run(&self, _: &str, u: &Understanding, cx: &Context) -> Reply {
        let Some(topic) = u.slots.get("TOPIC").filter(|t| !t.trim().is_empty()) else {
            return Reply::error("무엇에 대해 알고 싶으신지 잘 모르겠습니다.");
        };
        let topic = &complete_topic(topic, &cx.text);
        let Some(k) = &cx.knowledge else {
            return Reply::error("지식 저장소를 사용할 수 없습니다.");
        };
        if let Some(doc) = k.lookup(topic) {
            return Reply::say(answer(&doc, false));
        }
        match k.learn(topic, "질문") {
            Ok(Learned::Doc(doc)) => Reply::say(answer(&doc, true)),
            Ok(Learned::Ambiguous { title, options }) => {
                let list = if options.is_empty() {
                    String::new()
                } else {
                    format!("\n예: {}", options.join(", "))
                };
                Reply::say(format!("'{title}'은(는) 뜻이 여러 개입니다. 조금 더 정확한 이름으로 물어봐 주십시오.{list}"))
            }
            Ok(Learned::NotFound) => {
                if k.ask(Question::Define {
                    topic: topic.clone(),
                }) {
                    cx.emit(crate::Event::Question {
                        text: format!("'{topic}'에 대해 알려 주실 수 있습니까?"),
                    });
                }
                // 위키백과에 없으면 인터넷 검색으로
                if let Ok(Some(found)) = crate::skills::web::search_text(topic, cx) {
                    return Reply::say(format!(
                        "위키백과에는 '{topic}' 문서가 없어서 인터넷에서 찾아봤습니다.\n\n{found}"
                    ));
                }
                Reply::say(format!(
                    "'{topic}'에 대한 내용을 찾지 못했습니다. 학습 탭에서 무엇인지 알려 주시면 기억해 두겠습니다."
                ))
            }
            Err(e) => Reply::error(format!("'{topic}'에 대해 알아보지 못했습니다. {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::complete_topic;
    #[test]
    fn topic_completion() {
        assert_eq!(
            complete_topic("양자컴", "양자컴퓨터가 뭔지 궁금해"),
            "양자컴퓨터"
        );
        assert_eq!(complete_topic("고양이", "고양이가 뭐야"), "고양이");
        assert_eq!(complete_topic("블랙홀", "블랙홀이 뭐야"), "블랙홀");
        assert_eq!(complete_topic("광합성", "광합성에 대해 알려줘"), "광합성");
        assert_eq!(complete_topic("세종", "세종대왕은 누구야?"), "세종대왕");
    }
}

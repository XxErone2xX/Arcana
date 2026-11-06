//! 대화: 명령이 아닌 말은 Arcana 대화 모델이 직접 대답을 만들어냄.
//! (대화 모델이 없으면 정해진 문장으로 대답)

use crate::{Context, Reply, Skill};
use arcana_core::Understanding;

pub struct Chat;

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
                    let r = t.reply(&cx.text);
                    if !r.trim().is_empty() {
                        return Reply::say(r.trim());
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

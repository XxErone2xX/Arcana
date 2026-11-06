//! 인사, 감사, 자기소개 같은 대화

use crate::{Context, Reply, Skill};
use arcana_core::Understanding;

pub struct Chat;

fn pick<'a>(opts: &[&'a str]) -> &'a str {
    // 랜덤 라이브러리 없이 시간으로 대충 고르기
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
        ""
    }
    fn run(&self, intent: &str, _: &Understanding, cx: &Context) -> Reply {
        let name = cx.cfg().name.clone();
        match intent {
            "greet" => Reply::say(pick(&[
                "어 왔어? 뭐 해줄까?",
                "ㅎㅇ 뭐 필요해?",
                "왔구나. 시킬 거 있어?",
            ])),
            "thanks" => Reply::say(pick(&["별말을 다 ㅋㅋ", "ㅇㅋ 언제든", "이 정도야 뭐"])),
            "bye" => Reply {
                text: "ㅂㅇ 나중에 또 불러.".into(),
                quit: true,
                ..Default::default()
            },
            "who_are_you" => Reply::say(format!(
                "나는 {name}, 네 PC 비서야. 왼쪽 위 ☰ 눌러보면 할 수 있는 거 다 나와."
            )),
            _ => Reply::say(pick(&[
                "그건 아직 못 배웠어.",
                "음.. 무슨 말인지 모르겠어. (\"뭐 할 수 있어?\"라고 물어봐)",
                "그건 내 능력 밖이야 아직은.",
            ])),
        }
    }
}

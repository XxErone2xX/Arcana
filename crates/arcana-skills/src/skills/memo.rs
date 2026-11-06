//! 메모: %APPDATA%\Arcana\memos.json 에 저장

use crate::{Context, Reply, Skill};
use arcana_core::Understanding;
use serde::{Deserialize, Serialize};

pub struct Memo;

#[derive(Serialize, Deserialize)]
struct Item {
    text: String,
    time: String,
}

fn load(cx: &Context) -> Vec<Item> {
    std::fs::read_to_string(cx.data_dir.join("memos.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(cx: &Context, m: &[Item]) -> std::io::Result<()> {
    std::fs::create_dir_all(&cx.data_dir)?;
    std::fs::write(
        cx.data_dir.join("memos.json"),
        serde_json::to_string_pretty(m).unwrap(),
    )
}

impl Skill for Memo {
    fn intents(&self) -> &[&'static str] {
        &["memo_add", "memo_read", "memo_clear"]
    }
    fn describe(&self) -> &'static str {
        "메모 — \"내일 체육복 챙기기 메모해줘\", \"메모 보여줘\", \"메모 다 지워\""
    }
    fn run(&self, intent: &str, u: &Understanding, cx: &Context) -> Reply {
        let mut m = load(cx);
        match intent {
            "memo_add" => {
                let Some(t) = u.slots.get("MEMO").filter(|t| !t.is_empty()) else {
                    return Reply::error("어떤 내용을 메모할지 잘 모르겠습니다.");
                };
                m.push(Item {
                    text: t.clone(),
                    time: chrono::Local::now().format("%m/%d %H:%M").to_string(),
                });
                match save(cx, &m) {
                    Ok(_) => Reply::say(format!("'{t}' 메모했습니다.")),
                    Err(e) => Reply::error(format!("저장하지 못했습니다. ({e})")),
                }
            }
            "memo_read" if m.is_empty() => Reply::say("저장된 메모가 없습니다."),
            "memo_read" => Reply::say(format!(
                "메모가 {}개 있습니다.\n{}",
                m.len(),
                m.iter()
                    .enumerate()
                    .map(|(i, x)| format!("{}. {}  ({})", i + 1, x.text, x.time))
                    .collect::<Vec<_>>()
                    .join("\n")
            )),
            _ => Reply::ask(format!("메모 {}개를 전부 지울까요?", m.len())),
        }
    }
    fn confirmed(&self, _: &str, cx: &Context) -> Reply {
        match save(cx, &[]) {
            Ok(_) => Reply::say("메모를 모두 지웠습니다."),
            Err(e) => Reply::error(format!("지우지 못했습니다. ({e})")),
        }
    }
}

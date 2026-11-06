use crate::{Context, Reply, Skill};
use arcana_core::Understanding;

pub struct Web;

impl Skill for Web {
    fn intents(&self) -> &[&'static str] {
        &["web_search", "youtube"]
    }

    fn describe(&self) -> &'static str {
        "검색·유튜브 — \"파이썬 정렬 검색해줘\", \"유튜브에서 로파이 틀어줘\""
    }

    fn run(&self, intent: &str, u: &Understanding, cx: &Context) -> Reply {
        let q = u
            .slots
            .get("QUERY")
            .cloned()
            .unwrap_or_else(|| cx.text.clone());
        let enc = urlencoding::encode(&q);
        let (url, msg) = if intent == "youtube" {
            (
                format!("https://www.youtube.com/results?search_query={enc}"),
                format!("유튜브에서 '{q}' 검색 결과를 열었습니다."),
            )
        } else {
            (
                format!("https://www.google.com/search?q={enc}"),
                format!("'{q}' 검색 결과를 열었습니다."),
            )
        };
        match open::that_detached(url) {
            Ok(_) => Reply::say(msg),
            Err(e) => Reply::error(format!("브라우저를 열지 못했습니다. ({e})")),
        }
    }
}

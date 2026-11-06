//! 채팅에서 웹 탐색
//! - "파이썬 정렬 검색해줘", "인터넷에서 블랙홀 찾아줘" → 채팅 안에 검색 결과 + 1등 페이지 요약 (출처 전부 표시)
//! - 주소를 붙여넣으면 (https://...) → 그 페이지를 읽고 제목·요약·소제목·링크 + 출처
//! - "유튜브에서 ○○ 틀어줘" → 브라우저로 유튜브 검색

use crate::{Context, Reply, Skill};
use arcana_core::Understanding;

pub struct Web;

/// 검색 결과를 글로 (결과 없으면 Ok(None))
pub fn search_text(q: &str, cx: &Context) -> Result<Option<String>, String> {
    let Some(k) = &cx.knowledge else {
        return Err("웹 탐색을 사용할 수 없습니다.".into());
    };
    let (key, blocked) = {
        let c = cx.cfg();
        (c.brave_api_key.clone(), c.blocked_domains.clone())
    };
    let google = format!("https://www.google.com/search?q={}", urlencoding::encode(q));
    let hits = k.search(q, &key)?;
    if hits.is_empty() {
        return Ok(None);
    }
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        out.push_str(&format!(
            "{}{}. {}\n",
            if i == 0 { "" } else { "\n" },
            i + 1,
            h.title
        ));
        if !h.snippet.is_empty() {
            out.push_str(&format!("{}\n", h.snippet));
        }
        out.push_str(&format!("출처: {} — {}\n", h.source, h.url));
    }
    // 1등 결과 페이지에 직접 들어가서 내용 요약
    if let Some(top) = hits.iter().find(|h| !h.url.is_empty()) {
        if let Ok(p) = k.read_page(&top.url, &blocked) {
            if p.summary.chars().count() >= 20 && p.summary != top.snippet {
                out.push_str(&format!(
                    "\n'{}' 페이지 내용 요약:\n{}\n출처: {} — {}\n",
                    p.title, p.summary, p.site, p.url
                ));
            }
        }
    }
    let provider = if key.trim().is_empty() {
        "위키백과·DuckDuckGo"
    } else {
        "Brave Search"
    };
    out.push_str(&format!(
        "\n(검색: {provider}) 브라우저에서 더 보기: {google}"
    ));
    Ok(Some(out))
}

fn search_reply(q: &str, cx: &Context) -> Reply {
    let google = format!("https://www.google.com/search?q={}", urlencoding::encode(q));
    match search_text(q, cx) {
        Ok(Some(t)) => Reply::say(format!("'{q}' 검색 결과입니다.\n\n{t}")),
        Ok(None) => Reply::say(format!(
            "'{q}'에 대한 결과를 찾지 못했습니다.\n브라우저에서 보기: {google}"
        )),
        Err(e) => Reply::error(format!(
            "검색하지 못했습니다. {e}\n브라우저에서 보기: {google}"
        )),
    }
}

fn page_reply(url: &str, cx: &Context) -> Reply {
    let Some(k) = &cx.knowledge else {
        return Reply::error("웹 탐색을 사용할 수 없습니다.");
    };
    let blocked = cx.cfg().blocked_domains.clone();
    match k.read_page(url, &blocked) {
        Ok(p) => {
            let mut out = format!(
                "{}\n",
                if p.title.is_empty() {
                    &p.site
                } else {
                    &p.title
                }
            );
            if !p.summary.is_empty() {
                out.push_str(&format!("\n{}\n", p.summary));
            }
            if !p.headings.is_empty() {
                out.push_str(&format!("\n주요 내용: {}\n", p.headings.join(" · ")));
            }
            if !p.links.is_empty() {
                out.push_str("\n페이지 안의 링크:\n");
                for (u, t) in &p.links {
                    out.push_str(&format!("- {t} — {u}\n"));
                }
            }
            out.push_str(&format!("\n출처: {} — {}", p.site, p.url));
            Reply::say(out)
        }
        Err(e) => Reply::error(format!("페이지를 읽지 못했습니다. {e}\n주소: {url}")),
    }
}

impl Skill for Web {
    fn intents(&self) -> &[&'static str] {
        &["web_search", "youtube", "read_page"]
    }

    fn describe(&self) -> &'static str {
        "웹 — \"파이썬 정렬 검색해줘\", 주소 붙여넣기(페이지 읽기), \"유튜브에서 로파이 틀어줘\""
    }

    fn run(&self, intent: &str, u: &Understanding, cx: &Context) -> Reply {
        match intent {
            "read_page" => page_reply(
                u.slots.get("URL").map(String::as_str).unwrap_or(&cx.text),
                cx,
            ),
            "web_search" => search_reply(
                u.slots.get("QUERY").map(String::as_str).unwrap_or(&cx.text),
                cx,
            ),
            _ => {
                let q = u
                    .slots
                    .get("QUERY")
                    .cloned()
                    .unwrap_or_else(|| cx.text.clone());
                let url = format!(
                    "https://www.youtube.com/results?search_query={}",
                    urlencoding::encode(&q)
                );
                match open::that_detached(&url) {
                    Ok(_) => Reply::say(format!(
                        "유튜브에서 '{q}' 검색 결과를 열었습니다.\n출처: YouTube — {url}"
                    )),
                    Err(e) => Reply::error(format!("브라우저를 열지 못했습니다. ({e})")),
                }
            }
        }
    }
}

/// 문장 안의 웹 주소 찾기 ("이거 읽어줘 https://a.com/b" → "https://a.com/b")
pub fn find_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| "()[]<>\"'“”‘’,.".contains(c)))
        .find(|w| {
            (w.starts_with("http://") || w.starts_with("https://") || w.starts_with("www."))
                && w.contains('.')
                && w.len() > 8
        })
        .map(String::from)
}

#[cfg(test)]
mod tests {
    #[test]
    fn urls() {
        assert_eq!(
            super::find_url("이거 읽어줘 https://a.com/b?x=1").as_deref(),
            Some("https://a.com/b?x=1")
        );
        assert_eq!(
            super::find_url("(www.rust-lang.org) 요약해줘").as_deref(),
            Some("www.rust-lang.org")
        );
        assert_eq!(super::find_url("파이썬 검색해줘"), None);
    }
}

//! 웹 탐색 도구: robots.txt 규칙, HTML에서 제목·요약·링크 뽑기, 위험한 곳 거르기
//!
//! 지키는 규칙 (예의 바른 크롤러):
//! - robots.txt 에서 막은 곳은 안 들어감 (Crawl-delay 도 따름)
//! - 사이트가 robots.txt 로 Crawl-delay 를 정해 두면 따름, 하루 사이트당 횟수 제한 (knowledge.rs)
//! - 글 전체가 아니라 제목 + 짧은 요약 + 링크만 저장 (검색엔진처럼)
//! - 성인·도박·불법 사이트, 로그인/결제 페이지, 파일 다운로드는 건너뜀

use scraper::{Html, Selector};
use url::Url;

// ───────────────────────── robots.txt ─────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Robots {
    /// (허용 여부, 경로 패턴)
    rules: Vec<(bool, String)>,
    pub crawl_delay: Option<u64>,
    /// robots.txt 가 401/403 이면 전부 금지
    pub deny_all: bool,
}

impl Robots {
    pub fn allow_all() -> Self {
        Self::default()
    }

    pub fn deny_all() -> Self {
        Self {
            deny_all: true,
            ..Default::default()
        }
    }

    /// 우리 이름(ArcanaAssistant) 그룹이 있으면 그걸, 없으면 * 그룹을 따름
    pub fn parse(txt: &str, agent: &str) -> Self {
        let agent = agent.to_lowercase();
        let mut groups: Vec<(Vec<String>, Robots)> = Vec::new();
        let mut in_rules = false;
        for raw in txt.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let (k, v) = (k.trim().to_lowercase(), v.trim().to_string());
            match k.as_str() {
                "user-agent" => {
                    if in_rules || groups.is_empty() {
                        groups.push((vec![], Robots::default()));
                        in_rules = false;
                    }
                    groups.last_mut().unwrap().0.push(v.to_lowercase());
                }
                "allow" | "disallow" if !groups.is_empty() => {
                    in_rules = true;
                    if !(k == "disallow" && v.is_empty()) {
                        groups.last_mut().unwrap().1.rules.push((k == "allow", v));
                    }
                }
                "crawl-delay" if !groups.is_empty() => {
                    in_rules = true;
                    groups.last_mut().unwrap().1.crawl_delay =
                        v.parse::<f64>().ok().map(|d| d.ceil() as u64);
                }
                _ => {}
            }
        }
        let mine = groups
            .iter()
            .find(|(names, _)| names.iter().any(|n| n != "*" && agent.contains(n.as_str())));
        let star = groups
            .iter()
            .find(|(names, _)| names.iter().any(|n| n == "*"));
        mine.or(star).map(|(_, r)| r.clone()).unwrap_or_default()
    }

    /// 경로(+쿼리)가 허용되는지. 가장 길게 맞는 규칙이 이김, 같으면 허용 우선
    pub fn allowed(&self, path: &str) -> bool {
        if self.deny_all {
            return false;
        }
        let mut best: Option<(usize, bool)> = None;
        for (allow, pat) in &self.rules {
            if pattern_match(pat, path) {
                let len = pat.len();
                if best.map_or(true, |(l, a)| len > l || (len == l && *allow && !a)) {
                    best = Some((len, *allow));
                }
            }
        }
        best.map_or(true, |(_, a)| a)
    }
}

/// robots.txt 패턴: * = 아무 글자, 끝의 $ = 정확히 끝
fn pattern_match(pat: &str, path: &str) -> bool {
    let (pat, anchored) = match pat.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pat, false),
    };
    let parts: Vec<&str> = pat.split('*').collect();
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else {
            // $로 끝나는 패턴의 마지막 조각은 맨 뒤에서 찾음
            let found = if last && anchored {
                path[pos..].rfind(part)
            } else {
                path[pos..].find(part)
            };
            match found {
                Some(f) => pos += f + part.len(),
                None => return false,
            }
        }
    }
    !anchored || pos == path.len()
}

// ───────────────────────── 거르기 ─────────────────────────

const BLOCKED_TLDS: &[&str] = &[
    ".xxx", ".adult", ".porn", ".sex", ".casino", ".bet", ".poker", ".onion",
];
/// 주소에 이런 말이 있으면 안 감: 19금(성인)·불법 도박 사이트만
const BLOCKED_WORDS: &[&str] = &[
    "porn", "xxx", "hentai", "nsfw", "escort", "야동", "성인용", "19금", "섹스",
    "casino", "betting", "bet365", "카지노", "토토", "바카라", "먹튀", "슬롯머신",
];
/// 내용(제목·요약)에 이런 말이 있으면 저장 안 함 — 일반 문서("도박의 역사" 등)는 막지 않게 사이트 광고 말만
const BLOCKED_CONTENT: &[&str] = &[
    "porn", "xxx", "hentai", "야동", "성인용", "19금", "섹스",
    "토토사이트", "카지노사이트", "바카라사이트", "먹튀검증", "온라인카지노", "사설토토",
];
/// 막는 게 아니라, 읽을 내용이 없는 페이지 (로그인·결제·편집 화면, 끝없는 검색 결과)
const SKIP_PATH_WORDS: &[&str] = &[
    "login", "signin", "signup", "logout", "checkout", "payment", "password", "wp-admin",
    "/search", "?search", "action=edit", "special:",
];
const SKIP_EXT: &[&str] = &[
    ".pdf", ".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".zip", ".rar", ".7z", ".exe",
    ".msi", ".dmg", ".mp3", ".mp4", ".avi", ".mov", ".apk", ".iso", ".doc", ".docx", ".xls",
    ".xlsx", ".ppt", ".pptx",
];

/// 방문해도 되는 주소인지 (사이트 차단 목록 포함)
pub fn url_ok(u: &Url, blocked_domains: &[String]) -> bool {
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = u.host_str() else {
        return false;
    };
    let host = host.to_lowercase();
    if host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() {
        return false; // 내 컴퓨터·공유기 같은 내부 주소는 안 감
    }
    if BLOCKED_TLDS.iter().any(|t| host.ends_with(t)) {
        return false;
    }
    if blocked_domains.iter().any(|d| {
        let d = d.trim().to_lowercase();
        !d.is_empty() && (host == d || host.ends_with(&format!(".{d}")))
    }) {
        return false;
    }
    let full = u.as_str().to_lowercase();
    if BLOCKED_WORDS.iter().any(|w| full.contains(w))
        || SKIP_PATH_WORDS.iter().any(|w| full.contains(w))
    {
        return false;
    }
    let path = u.path().to_lowercase();
    !SKIP_EXT.iter().any(|e| path.ends_with(e))
}

/// 내용이 괜찮은지 (제목·요약에 위험한 단어가 있으면 저장 안 함)
pub fn content_ok(title: &str, summary: &str) -> bool {
    let t = format!("{title} {summary}").to_lowercase();
    !BLOCKED_CONTENT.iter().any(|w| t.contains(w))
}

/// 같은 페이지를 다른 주소로 두 번 읽지 않게 정리 (#뒤 제거, 끝의 / 정리)
pub fn canonical(u: &Url) -> String {
    let mut u = u.clone();
    u.set_fragment(None);
    let s = u.to_string();
    s.strip_suffix('/')
        .filter(|x| x.matches('/').count() > 2)
        .map(String::from)
        .unwrap_or(s)
}

// ───────────────────────── HTML 읽기 ─────────────────────────

pub struct Page {
    pub title: String,
    pub summary: String,
    pub lang: String,
    pub links: Vec<Url>,
    /// 링크 글자 (links 와 같은 순서)
    pub link_texts: Vec<String>,
    /// 페이지 소제목들 (h2, h3)
    pub headings: Vec<String>,
    /// 본문 전체 (메뉴·광고·스크립트 빼고). 요약하지 않음
    pub text: String,
    /// <meta name="robots" content="noindex"> 면 저장 안 함
    pub noindex: bool,
    pub nofollow: bool,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap()
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 본문 전체 뽑기: article/main 이 있으면 그 안, 없으면 body. 메뉴·머리말·꼬리말·옆칸·스크립트는 뺌
fn main_text(doc: &Html) -> String {
    const MAX: usize = 200_000;
    let root = ["article", "main", "[role=main]", "#content", "#mw-content-text", "body"]
        .iter()
        .find_map(|q| doc.select(&sel(q)).next());
    let Some(root) = root else { return String::new() };
    let skip = ["nav", "header", "footer", "aside", "script", "style", "noscript", "form", "button", "select", "template", "svg"];
    let blocks = sel("h1, h2, h3, h4, h5, p, li, blockquote, pre, td, th, dd, dt, figcaption");
    let mut out = String::new();
    let mut last = String::new();
    for el in root.select(&blocks) {
        // 안쪽 블록이 따로 또 잡히는 것(li 안의 p 등)과 메뉴 안의 글은 건너뜀
        let mut bad = false;
        let mut nested = false;
        for anc in el.ancestors().filter_map(scraper::ElementRef::wrap) {
            if anc.id() == root.id() {
                break;
            }
            let n = anc.value().name();
            if skip.contains(&n) {
                bad = true;
                break;
            }
            if matches!(n, "p" | "li" | "blockquote" | "td" | "th" | "dd" | "pre") {
                nested = true;
            }
        }
        if bad || nested {
            continue;
        }
        let t = if el.value().name() == "pre" { el.text().collect::<String>() } else { clean(&el.text().collect::<String>()) };
        let t = t.trim();
        if t.chars().count() < 2 || t == last {
            continue;
        }
        out.push_str(t);
        out.push('\n');
        last = t.to_string();
        if out.len() > MAX {
            break;
        }
    }
    out.trim().to_string()
}

/// "블랙홀 - 위키백과, 우리 모두의 백과사전" → "블랙홀"
pub fn clean_title(t: &str) -> String {
    let t = clean(t);
    for sep in [" | ", " - ", " — ", " :: ", " · "] {
        if let Some((head, _)) = t.split_once(sep) {
            if head.chars().count() >= 2 {
                return head.trim().to_string();
            }
        }
    }
    t
}

pub fn parse_html(html: &str, base: &Url) -> Page {
    let doc = Html::parse_document(html);
    let meta = |name: &str| -> Option<String> {
        let s = sel(&format!("meta[name=\"{name}\"], meta[property=\"{name}\"]"));
        doc.select(&s)
            .filter_map(|m| m.value().attr("content"))
            .map(clean)
            .find(|c| !c.is_empty())
    };
    let title = meta("og:title")
        .or_else(|| {
            doc.select(&sel("title"))
                .next()
                .map(|t| t.text().collect::<String>())
        })
        .or_else(|| {
            doc.select(&sel("h1"))
                .next()
                .map(|t| t.text().collect::<String>())
        })
        .map(|t| clean_title(&t))
        .unwrap_or_default();
    let mut summary = meta("description")
        .or_else(|| meta("og:description"))
        .unwrap_or_default();
    if summary.chars().count() < 40 {
        // 설명이 없으면 본문 첫 문단들
        let mut body = String::new();
        for p in doc.select(&sel("article p, main p, p")) {
            let t = clean(&p.text().collect::<String>());
            if t.chars().count() >= 30 {
                body.push_str(&t);
                body.push(' ');
                if body.chars().count() > 400 {
                    break;
                }
            }
        }
        if body.chars().count() > summary.chars().count() {
            summary = body;
        }
    }
    let robots = meta("robots").unwrap_or_default().to_lowercase();
    let lang = doc
        .select(&sel("html"))
        .next()
        .and_then(|h| h.value().attr("lang"))
        .map(|l| l.split('-').next().unwrap_or("").to_lowercase())
        .unwrap_or_default();
    let mut links = Vec::new();
    let mut link_texts = Vec::new();
    if !robots.contains("nofollow") {
        for a in doc.select(&sel("a[href]")) {
            if a.value()
                .attr("rel")
                .map_or(false, |r| r.contains("nofollow"))
            {
                continue;
            }
            if let Some(u) = a.value().attr("href").and_then(|h| base.join(h).ok()) {
                links.push(u);
                link_texts.push(clean(&a.text().collect::<String>()));
            }
        }
    }
    let headings = doc
        .select(&sel("h2, h3"))
        .map(|h| clean(&h.text().collect::<String>()))
        .filter(|t| (2..60).contains(&t.chars().count()))
        .take(8)
        .collect();
    let text = main_text(&doc);
    Page {
        text,
        link_texts,
        headings,
        title,
        summary: crate::knowledge::shorten(&summary, 320),
        lang,
        links,
        noindex: robots.contains("noindex"),
        nofollow: robots.contains("nofollow"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_rules() {
        let txt = "User-agent: *\nDisallow: /private/\nDisallow: /*.php$\nAllow: /private/ok\nCrawl-delay: 5\n\nUser-agent: BadBot\nDisallow: /\n";
        let r = Robots::parse(txt, "ArcanaAssistant/0.5");
        assert!(r.allowed("/"));
        assert!(r.allowed("/news/1"));
        assert!(!r.allowed("/private/secret"));
        assert!(r.allowed("/private/ok/page"));
        assert!(!r.allowed("/index.php"));
        assert!(r.allowed("/index.php?x=1"));
        assert_eq!(r.crawl_delay, Some(5));
        let mine = Robots::parse(
            "User-agent: arcanaassistant\nDisallow: /\n\nUser-agent: *\nAllow: /\n",
            "ArcanaAssistant/0.5",
        );
        assert!(!mine.allowed("/a"));
        assert!(Robots::parse("User-agent: *\nDisallow:\n", "x").allowed("/anything"));
        assert!(!Robots::deny_all().allowed("/"));
    }

    #[test]
    fn filters() {
        let ok = |s: &str| url_ok(&Url::parse(s).unwrap(), &["blocked.com".to_string()]);
        assert!(ok("https://www.rust-lang.org/learn"));
        assert!(!ok("https://casino-best.com/"));
        assert!(!ok("https://site.xxx/"));
        assert!(!ok("https://a.blocked.com/page"));
        assert!(!ok("https://example.com/login"));
        assert!(!ok("https://example.com/file.pdf"));
        assert!(!ok("http://192.168.0.1/"));
        assert!(!ok("ftp://example.com/"));
        assert!(content_ok("블랙홀", "중력이 매우 강한 천체"));
        assert!(!content_ok("최고의 온라인카지노 가입", ""));
        assert!(content_ok("도박 중독", "도박 중독은 도박을 멈추지 못하는 상태입니다"));
        assert!(ok("https://ko.wikipedia.org/wiki/암호_해독")); // 예전엔 crack·불법 같은 말로 막히던 일반 문서
        let p = parse_html("<html><body><nav><a href='/'>홈</a> 메뉴</nav><article><h1>제목</h1><p>첫 문단입니다.</p><ul><li><p>목록 안 문단</p></li></ul><script>x()</script></article><footer>저작권</footer></body></html>", &Url::parse("https://a.com/").unwrap());
        assert_eq!(p.text, "제목\n첫 문단입니다.\n목록 안 문단");
        assert_eq!(
            canonical(&Url::parse("https://a.com/b/#top").unwrap()),
            "https://a.com/b"
        );
        assert_eq!(
            canonical(&Url::parse("https://a.com/").unwrap()),
            "https://a.com/"
        );
    }

    #[test]
    fn html() {
        let html = r#"<html lang="ko-KR"><head><title>블랙홀 - 우주 백과</title>
            <meta name="description" content="블랙홀은 중력이 매우 강해서 빛조차 빠져나올 수 없는 천체입니다. 별이 죽으면서 생깁니다.">
            </head><body><a href="/space/star">별</a><a href="https://other.org/x#y">다른 곳</a>
            <a href="/ad" rel="nofollow">광고</a></body></html>"#;
        let base = Url::parse("https://space.example.com/bh").unwrap();
        let p = parse_html(html, &base);
        assert_eq!(p.title, "블랙홀");
        assert!(p.summary.starts_with("블랙홀은 중력이"));
        assert_eq!(p.lang, "ko");
        let links: Vec<String> = p.links.iter().map(canonical).collect();
        assert_eq!(
            links,
            vec![
                "https://space.example.com/space/star",
                "https://other.org/x"
            ]
        );
        assert!(!p.noindex);

        let p2 = parse_html("<html><head><meta name=robots content='noindex,nofollow'><title>x</title></head><body><p>본문입니다 본문입니다 본문입니다 본문입니다 본문입니다 본문입니다</p><a href='/a'>a</a></body></html>", &base);
        assert!(p2.noindex && p2.links.is_empty());
        assert!(p2.summary.contains("본문입니다"));
    }
}

//! 지식 저장소 ("공책") + 스스로 학습
//!
//! - 모르는 걸 물어보면 한국어 위키백과에서 찾아서 저장하고 대답
//! - 다음에 같은 걸 물으면 인터넷 없이 바로 대답
//! - 학습 탭에서 /Start 하면: 이미 아는 문서와 연결된 문서를 조금씩 읽어서 공책을 넓혀감
//! - 모르는 게 생기면 사용자에게 질문 → 대답을 기억
//!
//! 저장 위치: %APPDATA%\Arcana\knowledge\
//!   docs.jsonl       알게 된 문서들 (제목·짧은 요약·주소 — 대답할 때 찾아보는 용)
//!   fulltext.jsonl   문서 본문 전체 (대화 모델 학습 재료)
//!   frontier.json    다음에 읽을 문서 목록
//!   questions.json   사용자에게 물어볼 질문들
//!   taught.jsonl     사용자가 가르쳐 준 대답  (형식이 train_chat.py --data 와 같아서 재학습에도 쓸 수 있음)
//!   feedback.jsonl   대답에 대한 👍/👎 (+ 더 좋은 대답) → training/train_feedback.py 가 이걸로 학습
//!
//! 위키백과 내용은 CC BY-SA 라이선스라서 대답할 때 항상 출처를 같이 보여줌.

use crate::crawler::{self, Robots};
use crate::net;
use crate::{Config, Event, EventSink};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use url::Url;

const FRONTIER_MAX: usize = 5000;
const QUESTIONS_MAX: usize = 50;
const DAILY_LIMIT: usize = 1_000_000;
const MAX_HTML: u64 = 2 * 1024 * 1024;

/// 다른 언어 위키·일반 웹사이트 첫 시작점 (v0.5부터)
const SEEDS_WORLD: &[&str] = &[
    "wiki:en:Science",
    "wiki:en:History",
    "wiki:en:Computer science",
    "wiki:en:Music",
    "wiki:en:Space exploration",
    "wiki:en:Video game",
    "wiki:ja:科学",
    "wiki:ja:日本",
    "wiki:ja:コンピュータ",
];

/// 처음 공부를 시작할 문서들
const SEEDS: &[&str] = &[
    "대한민국",
    "컴퓨터",
    "인공지능",
    "수학",
    "물리학",
    "화학",
    "생물학",
    "역사",
    "지구",
    "태양계",
    "인터넷",
    "프로그래밍 언어",
    "음악",
    "미술",
    "축구",
    "비디오 게임",
    "한국어",
    "세계사",
    "경제",
    "철학",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    pub title: String,
    pub summary: String,
    pub url: String,
    /// "질문" (물어봐서) / "탐색" (스스로 공부) / "사용자" (가르쳐 줌)
    pub source: String,
    pub learned_at: String,
}

/// Arcana가 사용자에게 하는 질문
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Question {
    /// 대화에서 못 알아들은 말 → 어떻게 대답하면 좋을지
    Reply { phrase: String },
    /// 위키백과에 없는 말 → 뜻을 알려 달라
    Define { topic: String },
    /// 뜻이 여러 개인 말 → 어떤 걸 공부할지
    Choose { title: String, options: Vec<String> },
}

impl Question {
    pub fn text(&self) -> String {
        match self {
            Question::Reply { phrase } => {
                format!("대화 중에 '{phrase}'라는 말을 잘 이해하지 못했습니다. 이럴 땐 어떻게 대답하면 좋을까요?")
            }
            Question::Define { topic } => {
                format!("'{topic}'에 대한 문서를 찾지 못했습니다. 무엇인지 알려 주실 수 있습니까?")
            }
            Question::Choose { title, options } => {
                let list = options
                    .iter()
                    .enumerate()
                    .map(|(i, o)| format!("{}. {o}", i + 1))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("'{title}'은(는) 뜻이 여러 개입니다. 어떤 걸 공부할까요? (번호 또는 이름)\n{list}")
            }
        }
    }
}

pub enum Learned {
    Doc(Doc),
    Ambiguous { title: String, options: Vec<String> },
    NotFound,
}

#[derive(Default, Serialize, Deserialize)]
struct Frontier {
    queue: VecDeque<String>,
}

/// 대답에 대한 평가 (3단계: 피드백 학습)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub user: String,
    pub reply: String,
    /// 1 = 👍, -1 = 👎
    pub rating: i8,
    /// 👎일 때 사용자가 알려 준 더 좋은 대답
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub better: Option<String>,
    pub time: String,
}

#[derive(Serialize, Deserialize)]
struct Taught {
    user: String,
    bot: String,
}

/// 탐색 설정 (config.json 에서 옴)
#[derive(Clone)]
pub struct CrawlSettings {
    pub wiki_langs: Vec<String>,
    pub explore_web: bool,
    pub blocked_domains: Vec<String>,
    pub site_delay_secs: u64,
    pub site_daily_limit: usize,
    /// 이 사이트에서만 공부 (/Start only)
    pub only: Option<String>,
    /// robots.txt 지키기
    pub obey_robots: bool,
}

impl CrawlSettings {
    pub fn from(c: &Config) -> Self {
        let only = Some(c.study_only.trim().to_string()).filter(|s| !s.is_empty());
        Self {
            wiki_langs: c.wiki_langs.clone(),
            explore_web: c.explore_web || only.is_some(),
            blocked_domains: c.blocked_domains.clone(),
            site_delay_secs: c.site_delay_secs,
            // 한 사이트만 공부할 땐 그 사이트를 하루에 더 많이 읽음
            site_daily_limit: if only.is_some() { c.site_daily_limit.max(5000) } else { c.site_daily_limit.max(1) },
            only,
            obey_robots: c.obey_robots,
        }
    }
}

/// 대기열 항목이 어느 사이트인지 ("블랙홀" → ko.wikipedia.org)
fn item_host(item: &str) -> String {
    match target(item) {
        Target::Wiki(lang, _) => format!("{lang}.wikipedia.org"),
        Target::Web(url) | Target::Refresh(url) => url::Url::parse(&url).ok().and_then(|u| u.host_str().map(|h| h.to_lowercase())).unwrap_or_default(),
    }
}

/// host 가 그 사이트(또는 그 하위 주소)인지
pub fn host_matches(host: &str, site: &str) -> bool {
    let host = host.trim_start_matches("www.");
    host == site || host.ends_with(&format!(".{site}"))
}

/// 사용자가 준 링크 → 공부 범위로 쓸 사이트 이름 ("https://ko.wikipedia.org/wiki/X" → "wikipedia.org")
pub fn site_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?.to_lowercase();
    let host = host.trim_start_matches("www.").trim_start_matches("m.").to_string();
    if host == "wikipedia.org" || host.ends_with(".wikipedia.org") {
        return Some("wikipedia.org".into()); // 위키백과는 언어 상관없이 위키백과 전체
    }
    Some(host)
}

/// 위키백과 문서 주소 → 대기열 항목 ("https://en.wikipedia.org/wiki/Black_hole" → "wiki:en:Black hole")
pub fn wiki_item_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?;
    let lang = host.strip_suffix(".wikipedia.org")?.trim_start_matches("m.").split('.').next()?.to_string();
    let title = u.path().strip_prefix("/wiki/")?;
    let title = urlencoding::decode(title).ok()?.replace('_', " ");
    (!title.is_empty() && !title.contains(':')).then(|| wiki_key(&lang, &title))
}

/// 다음에 읽을 곳: "블랙홀"(한국어 위키) / "wiki:en:Black hole" / "https://..."
enum Target {
    Wiki(String, String),
    Web(String),
    /// 예전에 읽은 페이지 다시 확인 (사이트 내용이 바뀌었을 수 있음)
    Refresh(String),
}

fn target(s: &str) -> Target {
    if let Some(u) = s.strip_prefix("refresh:") {
        return Target::Refresh(u.into());
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        return Target::Web(s.into());
    }
    if let Some(rest) = s.strip_prefix("wiki:") {
        if let Some((lang, title)) = rest.split_once(':') {
            return Target::Wiki(lang.into(), title.into());
        }
    }
    Target::Wiki("ko".into(), s.into())
}

fn wiki_key(lang: &str, title: &str) -> String {
    if lang == "ko" {
        title.to_string()
    } else {
        format!("wiki:{lang}:{title}")
    }
}

enum Step {
    Learned(String, String),
    Skip,
    Later,
}

/// 탐색 한 걸음 결과
pub enum Studied {
    Learned {
        title: String,
        site: String,
    },
    /// 사이트 간격 때문에 잠깐 기다려야 함
    Wait,
    /// 읽을 게 없거나 오늘 한도
    Idle,
}

/// 채팅에서 읽은 웹페이지
#[derive(Debug, Clone)]
pub struct WebPage {
    pub title: String,
    pub summary: String,
    pub headings: Vec<String>,
    /// (주소, 링크 글자)
    pub links: Vec<(String, String)>,
    pub url: String,
    pub site: String,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub snippet: String,
    pub url: String,
    /// 어디서 찾았는지 (위키백과, DuckDuckGo, Brave Search)
    pub source: String,
}

/// "<span class=searchmatch>블랙홀</span>은" → "블랙홀은"
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

pub struct Knowledge {
    dir: PathBuf,
    robots: Mutex<HashMap<String, Robots>>,
    site_last: Mutex<HashMap<String, Instant>>,
    site_today: Mutex<(String, HashMap<String, usize>)>,
    /// 사이트별 연속 실패 횟수 (3번이면 그날은 건너뜀)
    site_fail: Mutex<HashMap<String, usize>>,
    /// 사이트별 마지막 문제 ("접근 거부(봇 차단)" 등) — 한 사이트만 공부할 때 이유를 알려 주려고
    site_problem: Mutex<HashMap<String, String>>,
    /// "로딩 중" 페이지를 몇 번 다시 시도했는지
    loading_tries: Mutex<HashMap<String, u8>>,
    /// 사이트맵을 이미 훑은 사이트 (하루에 한 번)
    sitemap_done: Mutex<HashMap<String, String>>,
    /// 페이지 내용 지문 (주소 → 해시). 다시 읽었을 때 바뀌었는지 보려고. page_hashes.json
    page_hash: Mutex<HashMap<String, u64>>,
    /// 다시 확인한 시각 (같은 페이지를 하루에 여러 번 확인하지 않게)
    refreshed: Mutex<HashMap<String, Instant>>,
    /// 파일에 한 줄씩 쓸 때 여러 일꾼의 줄이 섞이지 않게
    io_lock: Mutex<()>,
    add_lock: Mutex<()>,
    frontier_saved: Mutex<Option<Instant>>,
    /// 위키백과 API 를 동시에 부르는 수 (위키미디어 이용 수칙: 너무 많이 동시에 부르지 않기)
    wiki_inflight: Mutex<usize>,
    /// 여러 사이트에 걸친 연속 연결 실패 (5번이면 진짜 인터넷 문제로 보고 쉼)
    net_fail_streak: Mutex<usize>,
    /// /Start 할 때 쉬고 있던 공부를 바로 깨우는 신호
    wake: std::sync::atomic::AtomicU64,
    docs: RwLock<Vec<Doc>>,
    index: RwLock<HashMap<String, usize>>,
    frontier: Mutex<Frontier>,
    seen: Mutex<HashSet<String>>,
    questions: Mutex<VecDeque<Question>>,
    taught: RwLock<HashMap<String, String>>,
    feedback: RwLock<Vec<Feedback>>,
    /// 최근에 한 대답들 (같은 대답 반복 안 하게)
    recent: Mutex<VecDeque<String>>,
    today: Mutex<(String, usize)>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Stats {
    pub count: usize,
    pub queued: usize,
    pub taught: usize,
    pub questions: usize,
    pub likes: usize,
    pub dislikes: usize,
    pub recent: Vec<String>,
}

pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// 가르쳐 준 말 비교용: "뭐함 ㅋㅋ" = "뭐함" (ㅋㅋ, ㅠㅠ 같은 자음·모음만 있는 부분은 무시)
fn phrase_key(s: &str) -> String {
    let n = normalize(s);
    let core: String = n
        .chars()
        .filter(|c| !('\u{3131}'..='\u{318E}').contains(c))
        .collect();
    if core.is_empty() {
        n
    } else {
        core
    }
}

fn bigram_sim(a: &str, b: &str) -> f32 {
    let grams = |s: &str| -> HashSet<(char, char)> {
        let c: Vec<char> = s.chars().collect();
        c.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (ga, gb) = (grams(a), grams(b));
    if ga.is_empty() || gb.is_empty() {
        return if a == b { 1.0 } else { 0.0 };
    }
    2.0 * ga.intersection(&gb).count() as f32 / (ga.len() + gb.len()) as f32
}

pub fn shorten(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for sentence in text.split_inclusive(". ") {
        if !out.is_empty() && out.chars().count() + sentence.chars().count() > max_chars {
            break;
        }
        out.push_str(sentence);
    }
    if out.chars().count() > max_chars + 80 {
        out = out.chars().take(max_chars).collect::<String>() + "…";
    }
    out.trim().to_string()
}

fn worth_studying(title: &str) -> bool {
    if title.contains(':') || title.contains("목록") || title.chars().count() > 30 {
        return false;
    }
    let digits = title.chars().filter(|c| c.is_ascii_digit()).count();
    !(digits > 0
        && (title.ends_with('년')
            || title.ends_with('일')
            || title.ends_with("년대")
            || digits * 2 >= title.chars().count()))
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

impl Knowledge {
    pub fn open(dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_default();
        let docs: Vec<Doc> = read("docs.jsonl")
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let mut index = HashMap::new();
        let mut seen = HashSet::new();
        for (i, d) in docs.iter().enumerate() {
            index.insert(normalize(&d.title), i);
            seen.insert(d.title.clone());
        }
        let mut frontier: Frontier =
            serde_json::from_str(&read("frontier.json")).unwrap_or_default();
        if frontier.queue.is_empty() && docs.is_empty() {
            frontier.queue.extend(SEEDS.iter().map(|s| s.to_string()));
        }
        seen.extend(frontier.queue.iter().cloned());
        let questions: VecDeque<Question> =
            serde_json::from_str(&read("questions.json")).unwrap_or_default();
        let taught = read("taught.jsonl")
            .lines()
            .filter_map(|l| serde_json::from_str::<Taught>(l).ok())
            .map(|t| (phrase_key(&t.user), t.bot))
            .collect();
        let feedback = read("feedback.jsonl")
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        // v0.5: 다른 언어 위키·웹 시작점 한 번 추가
        if !dir.join("seeded_world").exists() {
            for s in SEEDS_WORLD {
                if seen.insert(s.to_string()) {
                    frontier.queue.push_back(s.to_string());
                }
            }
            let _ = std::fs::write(dir.join("seeded_world"), "1");
        }
        Arc::new(Self {
            robots: Mutex::new(HashMap::new()),
            site_last: Mutex::new(HashMap::new()),
            site_today: Mutex::new((String::new(), HashMap::new())),
            site_fail: Mutex::new(HashMap::new()),
            site_problem: Mutex::new(HashMap::new()),
            loading_tries: Mutex::new(HashMap::new()),
            refreshed: Mutex::new(HashMap::new()),
            io_lock: Mutex::new(()),
            add_lock: Mutex::new(()),
            frontier_saved: Mutex::new(None),
            wiki_inflight: Mutex::new(0),
            sitemap_done: Mutex::new(HashMap::new()),
            page_hash: Mutex::new(std::fs::read_to_string(dir.join("page_hashes.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()),
            net_fail_streak: Mutex::new(0),
            wake: std::sync::atomic::AtomicU64::new(0),
            dir,
            docs: RwLock::new(docs),
            index: RwLock::new(index),
            frontier: Mutex::new(frontier),
            seen: Mutex::new(seen),
            questions: Mutex::new(questions),
            taught: RwLock::new(taught),
            feedback: RwLock::new(feedback),
            recent: Mutex::new(VecDeque::new()),
            today: Mutex::new((String::new(), 0)),
        })
    }

    pub fn stats(&self) -> Stats {
        let docs = self.docs.read().unwrap();
        Stats {
            count: docs.len(),
            queued: self.frontier.lock().unwrap().queue.len(),
            taught: self.taught.read().unwrap().len(),
            questions: self.questions.lock().unwrap().len(),
            likes: self
                .feedback
                .read()
                .unwrap()
                .iter()
                .filter(|f| f.rating > 0)
                .count(),
            dislikes: self
                .feedback
                .read()
                .unwrap()
                .iter()
                .filter(|f| f.rating < 0)
                .count(),
            recent: docs.iter().rev().take(8).map(|d| d.title.clone()).collect(),
        }
    }

    // ───────────── 공책 ─────────────

    pub fn lookup(&self, topic: &str) -> Option<Doc> {
        let n = normalize(topic);
        if n.is_empty() {
            return None;
        }
        let docs = self.docs.read().unwrap();
        if let Some(&i) = self.index.read().unwrap().get(&n) {
            return Some(docs[i].clone());
        }
        docs.iter()
            .map(|d| (bigram_sim(&n, &normalize(&d.title)), d))
            .filter(|(s, _)| *s >= 0.8)
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .map(|(_, d)| d.clone())
    }

    fn add(&self, doc: Doc) -> Doc {
        let key = normalize(&doc.title);
        let _one = self.add_lock.lock().unwrap(); // 같은 문서를 두 일꾼이 동시에 넣지 않게
        if let Some(&i) = self.index.read().unwrap().get(&key) {
            return self.docs.read().unwrap()[i].clone();
        }
        {
            let _io = self.io_lock.lock().unwrap();
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.join("docs.jsonl"))
            {
                let _ = writeln!(f, "{}", serde_json::to_string(&doc).unwrap());
            }
        }
        let mut docs = self.docs.write().unwrap();
        docs.push(doc.clone());
        self.index.write().unwrap().insert(key, docs.len() - 1);
        self.seen.lock().unwrap().insert(doc.title.clone());
        doc
    }

    /// 본문 전체 저장 (요약하지 않음) → fulltext.jsonl. 대화 모델 1단계 학습 재료.
    /// 메모리에는 안 올리고 파일에만 씀
    fn save_text(&self, title: &str, url: &str, source: &str, text: &str) {
        let text = text.trim();
        if text.chars().count() < 200 {
            return;
        }
        let _io = self.io_lock.lock().unwrap();
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("fulltext.jsonl")) {
            let row = serde_json::json!({ "title": title, "url": url, "source": source, "text": text, "learned_at": now() });
            let _ = writeln!(f, "{row}");
        }
    }

    /// 위키백과 문서 본문 전체 (일반 글자로)
    fn wiki_fulltext(&self, lang: &str, title: &str) -> Option<String> {
        let url = format!(
            "{}/w/api.php?action=query&prop=extracts&explaintext=1&exsectionformat=plain&redirects=1&format=json&titles={}",
            Self::wiki(lang),
            urlencoding::encode(title)
        );
        let v = self.get_json(&url).ok()?;
        v["query"]["pages"].as_object()?.values().find_map(|p| p["extract"].as_str().map(String::from))
    }

    // ───────────── 위키백과 (여러 언어) ─────────────

    fn get_json(&self, url: &str) -> Result<serde_json::Value, String> {
        match net::get(url, &[("Accept", "application/json")]) {
            Ok(r) => r.into_json().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(404, _)) => Ok(serde_json::Value::Null),
            Err(e) => Err(format!(
                "{}. (/Diag 로 연결 상태를 확인할 수 있습니다)",
                net::describe(&e)
            )),
        }
    }

    fn wiki(lang: &str) -> String {
        format!("https://{lang}.wikipedia.org")
    }

    fn summary(
        &self,
        lang: &str,
        title: &str,
    ) -> Result<Option<(String, String, String, String)>, String> {
        let url = format!(
            "{}/api/rest_v1/page/summary/{}",
            Self::wiki(lang),
            urlencoding::encode(&title.replace(' ', "_"))
        );
        let v = self.get_json(&url)?;
        let extract = v["extract"].as_str().unwrap_or("").trim().to_string();
        if extract.is_empty() {
            return Ok(None);
        }
        Ok(Some((
            v["title"].as_str().unwrap_or(title).to_string(),
            extract,
            v["content_urls"]["desktop"]["page"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            v["type"].as_str().unwrap_or("standard").to_string(),
        )))
    }

    fn search_title(&self, lang: &str, q: &str) -> Result<Option<String>, String> {
        let url = format!(
            "{}/w/api.php?action=query&list=search&srlimit=1&format=json&srsearch={}",
            Self::wiki(lang),
            urlencoding::encode(q)
        );
        Ok(self.get_json(&url)?["query"]["search"][0]["title"]
            .as_str()
            .map(String::from))
    }

    /// 문서 안의 위키 링크(prop=links) 또는 바깥 링크(prop=extlinks)
    fn wiki_links(&self, lang: &str, title: &str, external: bool) -> Vec<String> {
        let (prop, field, extra) = if external {
            ("extlinks", "extlinks", "&ellimit=30")
        } else {
            ("links", "links", "&plnamespace=0&pllimit=60")
        };
        let url = format!(
            "{}/w/api.php?action=query&prop={prop}{extra}&format=json&titles={}",
            Self::wiki(lang),
            urlencoding::encode(title)
        );
        let Ok(v) = self.get_json(&url) else {
            return vec![];
        };
        let mut out = Vec::new();
        if let Some(pages) = v["query"]["pages"].as_object() {
            for p in pages.values() {
                for l in p[field].as_array().into_iter().flatten() {
                    // 위키 링크는 "title", 바깥 링크는 "*" 또는 "url"
                    if let Some(t) = l["title"]
                        .as_str()
                        .or(l["*"].as_str())
                        .or(l["url"].as_str())
                    {
                        out.push(t.to_string());
                    }
                }
            }
        }
        out
    }

    fn push_targets(&self, items: impl IntoIterator<Item = String>) {
        let mut seen = self.seen.lock().unwrap();
        let mut f = self.frontier.lock().unwrap();
        for t in items {
            if f.queue.len() < FRONTIER_MAX && seen.insert(t.clone()) {
                f.queue.push_back(t);
            }
        }
        drop(f);
        drop(seen);
        self.save_frontier();
    }

    /// 한 사이트만 공부할 때: 대기열이 가득 차 있어도 맨 앞에 넣음 (넘치면 맨 뒤 것을 버림)
    fn push_targets_front(&self, items: Vec<String>) {
        let mut seen = self.seen.lock().unwrap();
        let mut f = self.frontier.lock().unwrap();
        for t in items.into_iter().rev() {
            if seen.insert(t.clone()) {
                f.queue.push_front(t);
                while f.queue.len() > FRONTIER_MAX {
                    f.queue.pop_back();
                }
            }
        }
        drop(f);
        drop(seen);
        self.save_frontier();
    }

    fn save_hashes(&self) {
        let h = self.page_hash.lock().unwrap();
        let _ = std::fs::write(self.dir.join("page_hashes.json"), serde_json::to_string(&*h).unwrap_or_default());
    }

    /// 한 사이트만 공부하는데 읽을 링크가 떨어졌을 때:
    ///  1) 사이트맵(sitemap.xml)에서 아직 안 읽은 페이지들을 찾고 (하루 한 번)
    ///  2) 그래도 없으면 예전에 읽은 페이지를 다시 확인 (하루 지난 것, 내용이 바뀌었을 수 있음)
    /// 새로 넣은 개수를 돌려줌
    fn refill_site(&self, site: &str, s: &CrawlSettings) -> usize {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let checked = self.sitemap_done.lock().unwrap().get(site) == Some(&today);
        if !checked {
            self.sitemap_done.lock().unwrap().insert(site.to_string(), today.clone());
            // 그 사이트에서 읽은 주소가 있으면 거기서 (http/포트까지 맞게), 없으면 https://사이트/
            let base = self
                .docs
                .read()
                .unwrap()
                .iter()
                .rev()
                .filter_map(|d| Url::parse(&d.url).ok())
                .find(|u| host_matches(u.host_str().unwrap_or(""), site))
                .and_then(|u| u.join("/").ok())
                .map(|u| u.to_string())
                .unwrap_or_else(|| format!("https://{site}/"));
            let mut maps: Vec<String> = Url::parse(&base).map(|u| self.robots_for(&u).sitemaps).unwrap_or_default();
            maps.extend([format!("{base}sitemap.xml"), format!("{base}sitemap_index.xml")]);
            let mut pages = Vec::new();
            let mut fetched = 0;
            while let Some(m) = maps.pop() {
                if fetched >= 12 || pages.len() >= 2000 {
                    break;
                }
                fetched += 1;
                let Ok(resp) = net::get(&m, &[]) else { continue };
                let mut xml = String::new();
                let _ = std::io::Read::read_to_string(&mut std::io::Read::take(resp.into_reader(), 8 * 1024 * 1024), &mut xml);
                let (p, sub) = crawler::parse_sitemap(&xml);
                pages.extend(p);
                maps.extend(sub);
            }
            let items: Vec<String> = pages
                .into_iter()
                .filter_map(|p| Url::parse(&p).ok())
                .filter(|u| host_matches(u.host_str().unwrap_or(""), site) && crawler::url_ok(u, &s.blocked_domains))
                .map(|u| crawler::canonical(&u))
                .take(1000)
                .collect();
            let before = self.frontier.lock().unwrap().queue.len();
            self.push_targets_front(items);
            let added = self.frontier.lock().unwrap().queue.len().saturating_sub(before);
            if added > 0 {
                return added;
            }
        }
        // 예전에 읽은 페이지 다시 확인 (마지막으로 본 지 하루 넘은 것, 최대 30개)
        let cutoff = (chrono::Local::now() - chrono::Duration::hours(24)).format("%Y-%m-%d %H:%M").to_string();
        let recent = self.refreshed.lock().unwrap().clone();
        let olds: Vec<String> = self
            .docs
            .read()
            .unwrap()
            .iter()
            .filter(|d| d.learned_at < cutoff && Url::parse(&d.url).map_or(false, |u| host_matches(u.host_str().unwrap_or(""), site)))
            .filter(|d| recent.get(&d.url).map_or(true, |t| t.elapsed() > Duration::from_secs(24 * 3600)))
            .map(|d| format!("refresh:{}", d.url))
            .take(30)
            .collect();
        let mut f = self.frontier.lock().unwrap();
        let n = olds.iter().filter(|o| !f.queue.contains(o)).count();
        for o in olds {
            if !f.queue.contains(&o) {
                f.queue.push_back(o);
            }
        }
        n
    }

    fn note_problem(&self, host: &str, why: impl Into<String>) {
        self.site_problem.lock().unwrap().insert(host.to_string(), why.into());
    }

    /// 그 사이트에서 마지막으로 생긴 문제
    pub fn site_problem(&self, site: &str) -> Option<String> {
        self.site_problem.lock().unwrap().iter().find(|(h, _)| host_matches(h.split(':').next().unwrap_or(h), site)).map(|(_, w)| w.clone())
    }

    fn enqueue_wiki_links(&self, lang: &str, title: &str, explore_web: bool, blocked: &[String]) {
        let links = self.wiki_links(lang, title, false);
        let step = (links.len() / 10).max(1);
        let mut items: Vec<String> = links
            .into_iter()
            .step_by(step)
            .take(10)
            .filter(|t| worth_studying(t))
            .map(|t| wiki_key(lang, &t))
            .collect();
        if explore_web {
            // 위키 문서의 출처·바깥 링크 → 일반 웹으로 넘어가는 다리
            let ext: Vec<String> = self
                .wiki_links(lang, title, true)
                .into_iter()
                .filter_map(|u| {
                    Url::parse(&if u.starts_with("//") {
                        format!("https:{u}")
                    } else {
                        u
                    })
                    .ok()
                })
                .filter(|u| crawler::url_ok(u, blocked))
                .map(|u| crawler::canonical(&u))
                .take(3)
                .collect();
            items.extend(ext);
        }
        self.push_targets(items);
    }

    /// 쉬고 있던 공부를 바로 다시 시작 (/Start)
    pub fn wake(&self) {
        self.wake.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn wake_count(&self) -> u64 {
        self.wake.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 위키백과 아무 문서 n개 (위키백과에서만 공부할 때 읽을 게 떨어지면)
    fn random_wiki(&self, lang: &str, n: usize) -> Vec<String> {
        let url = format!("{}/w/api.php?action=query&list=random&rnnamespace=0&rnlimit={n}&format=json", Self::wiki(lang));
        self.get_json(&url)
            .ok()
            .and_then(|v| v["query"]["random"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|r| r["title"].as_str().map(|t| wiki_key(lang, t)))
            .collect()
    }

    /// 이 주소부터 공부 (대기열 맨 앞에)
    pub fn study_first(&self, url: &str) {
        // 다시 해 보라고 한 거니까, 오늘 실패 기록은 지움
        if let Ok(u) = url::Url::parse(url) {
            let h = Self::host_of(&u);
            self.site_fail.lock().unwrap().remove(&h);
            self.site_problem.lock().unwrap().remove(&h);
        }
        let is_wiki_site = site_of(url).as_deref() == Some("wikipedia.org");
        if is_wiki_site && wiki_item_of(url).is_none() {
            // 위키백과 첫 화면 같은 주소: 그 언어 위키의 문서들부터
            let lang = url::Url::parse(url).ok().and_then(|u| u.host_str().and_then(|h| h.strip_suffix(".wikipedia.org")).map(|l| l.trim_start_matches("m.").to_string())).unwrap_or_else(|| "ko".into());
            let items = self.random_wiki(&lang, 20);
            let mut f = self.frontier.lock().unwrap();
            for it in items.into_iter().rev() {
                f.queue.push_front(it);
            }
            drop(f);
            self.save_frontier();
            self.wake();
            return;
        }
        let item = wiki_item_of(url).unwrap_or_else(|| url::Url::parse(url).map(|u| crawler::canonical(&u)).unwrap_or_else(|_| url.to_string()));
        let mut f = self.frontier.lock().unwrap();
        f.queue.retain(|x| x != &item);
        f.queue.push_front(item);
        drop(f);
        self.save_frontier();
        self.wake();
    }

    /// 대기열 파일 저장. 일꾼들이 자주 부르므로 3초에 한 번만 실제로 씀
    fn save_frontier(&self) {
        {
            let mut last = self.frontier_saved.lock().unwrap();
            if last.map_or(false, |t| t.elapsed() < Duration::from_secs(3)) {
                return;
            }
            *last = Some(Instant::now());
        }
        let json = serde_json::to_string(&*self.frontier.lock().unwrap()).unwrap();
        let _ = std::fs::write(self.dir.join("frontier.json"), json);
    }

    fn learn_wiki(
        &self,
        lang: &str,
        topic: &str,
        source: &str,
        s: Option<&CrawlSettings>,
    ) -> Result<Learned, String> {
        let mut found = self.summary(lang, topic)?;
        if found.is_none() {
            if let Some(t) = self.search_title(lang, topic)? {
                found = self.summary(lang, &t)?;
            }
        }
        let Some((title, extract, url, kind)) = found else {
            return Ok(Learned::NotFound);
        };
        if kind == "disambiguation" {
            let options: Vec<String> = self
                .wiki_links(lang, &title, false)
                .into_iter()
                .filter(|t| worth_studying(t))
                .take(6)
                .collect();
            return Ok(Learned::Ambiguous { title, options });
        }
        let known = self.index.read().unwrap().contains_key(&normalize(&title));
        if !known {
            if let Some(full) = self.wiki_fulltext(lang, &title) {
                self.save_text(&title, &url, source, &full);
            }
        }
        let doc = self.add(Doc {
            title: title.clone(),
            summary: shorten(&extract, 320),
            url,
            source: source.into(),
            learned_at: now(),
        });
        let (web, blocked) = s.map_or((false, vec![]), |s| {
            (s.explore_web, s.blocked_domains.clone())
        });
        self.enqueue_wiki_links(lang, &title, web, &blocked);
        Ok(Learned::Doc(doc))
    }

    /// 물어본 걸 배워오기: 한국어 위키 → 없으면 다른 언어 위키 순서로
    pub fn learn(&self, topic: &str, source: &str) -> Result<Learned, String> {
        let r = self.learn_wiki("ko", topic, source, None)?;
        if !matches!(r, Learned::NotFound) {
            return Ok(r);
        }
        for lang in ["en", "ja"] {
            if let Ok(Learned::Doc(d)) = self.learn_wiki(lang, topic, source, None) {
                return Ok(Learned::Doc(d));
            }
        }
        Ok(Learned::NotFound)
    }

    // ───────────── 일반 웹 ─────────────

    /// 사이트 구분용 (포트 포함: a.com, a.com:8080)
    fn host_of(u: &Url) -> String {
        let h = u.host_str().unwrap_or("").to_lowercase();
        match u.port() {
            Some(p) => format!("{h}:{p}"),
            None => h,
        }
    }

    fn robots_for(&self, u: &Url) -> Robots {
        let host = Self::host_of(u);
        if let Some(r) = self.robots.lock().unwrap().get(&host) {
            return r.clone();
        }
        let Ok(url) = u.join("/robots.txt") else {
            return Robots::deny_all();
        };
        let r = match net::get(url.as_str(), &[]) {
            Ok(resp) => {
                let mut txt = String::new();
                let _ = std::io::Read::read_to_string(
                    &mut std::io::Read::take(resp.into_reader(), 512 * 1024),
                    &mut txt,
                );
                Robots::parse(&txt, net::USER_AGENT)
            }
            Err(ureq::Error::Status(401 | 403, _)) => Robots::deny_all(),
            Err(ureq::Error::Status(_, _)) => Robots::allow_all(), // 404 등: 규칙 없음
            Err(_) => Robots::deny_all(),                          // 연결 실패: 이번엔 건너뜀
        };
        self.robots.lock().unwrap().insert(host, r.clone());
        r
    }

    fn crawl(&self, raw: &str, s: &CrawlSettings, refresh: bool) -> Result<Step, String> {
        let Ok(u) = Url::parse(raw) else {
            return Ok(Step::Skip);
        };
        if !crawler::url_ok(&u, &s.blocked_domains) {
            return Ok(Step::Skip);
        }
        let host = Self::host_of(&u);
        // 하루 사이트당 한도
        {
            let mut t = self.site_today.lock().unwrap();
            let d = chrono::Local::now().format("%Y-%m-%d").to_string();
            if t.0 != d {
                *t = (d, HashMap::new());
                self.site_fail.lock().unwrap().clear();
            }
            if *t.1.get(&host).unwrap_or(&0) >= s.site_daily_limit {
                return Ok(Step::Skip);
            }
        }
        if *self.site_fail.lock().unwrap().get(&host).unwrap_or(&0) >= 3 {
            return Ok(Step::Skip); // 오늘 계속 실패한 사이트
        }
        // 같은 사이트 간격 (robots.txt 를 안 지킬 땐 최소 1초 — 사이트에 부담을 주거나 차단당하지 않게)
        let delay = if s.obey_robots {
            let robots = self.robots_for(&u);
            let path = match u.query() {
                Some(q) => format!("{}?{q}", u.path()),
                None => u.path().to_string(),
            };
            if !robots.allowed(&path) {
                return Ok(Step::Skip);
            }
            Duration::from_secs(robots.crawl_delay.unwrap_or(0).max(s.site_delay_secs).min(120))
        } else {
            Duration::from_secs(s.site_delay_secs.max(1))
        };
        // 같은 사이트를 여러 일꾼이 동시에 읽지 않게: 확인과 기록을 한 번에
        {
            let mut last = self.site_last.lock().unwrap();
            if last.get(&host).map_or(false, |t| t.elapsed() < delay) {
                return Ok(Step::Later);
            }
            last.insert(host.clone(), Instant::now());
        }
        *self
            .site_today
            .lock()
            .unwrap()
            .1
            .entry(host.clone())
            .or_insert(0) += 1;

        let resp = match net::get(u.as_str(), &[]) {
            Ok(r) => {
                *self.net_fail_streak.lock().unwrap() = 0;
                self.site_fail.lock().unwrap().remove(&host);
                r
            }
            Err(ureq::Error::Status(code, _)) => {
                *self.net_fail_streak.lock().unwrap() = 0; // 서버가 대답은 했으니 인터넷은 됨
                self.note_problem(&host, match code {
                    403 | 401 => format!("사이트가 접근을 거부했습니다 (HTTP {code}, 자동 수집 차단)"),
                    429 => "너무 자주 읽어서 사이트가 잠시 막았습니다 (HTTP 429)".to_string(),
                    404 => "없는 페이지입니다 (HTTP 404)".to_string(),
                    _ => format!("사이트 오류 (HTTP {code})"),
                });
                return Ok(Step::Skip);
            }
            Err(e) => {
                // 이 페이지·사이트만의 문제(리다이렉트 반복, 잘못된 응답, 꺼진 서버 등)는 건너뛰고 계속
                self.note_problem(&host, net::describe(&e));
                *self
                    .site_fail
                    .lock()
                    .unwrap()
                    .entry(host.clone())
                    .or_insert(0) += 1;
                let mut streak = self.net_fail_streak.lock().unwrap();
                *streak += 1;
                if *streak >= 5 {
                    *streak = 0;
                    return Err(net::describe(&e)); // 여러 사이트가 연속으로 실패 → 인터넷 문제
                }
                return Ok(Step::Skip);
            }
        };
        if !resp.content_type().contains("html") {
            self.note_problem(&host, "웹 문서(HTML)가 아닙니다");
            return Ok(Step::Skip);
        }
        let final_url = Url::parse(resp.get_url()).unwrap_or(u);
        let mut html = String::new();
        if std::io::Read::read_to_string(
            &mut std::io::Read::take(resp.into_reader(), MAX_HTML),
            &mut html,
        )
        .is_err()
        {
            return Ok(Step::Skip);
        }
        let page = crawler::parse_html(&html, &final_url);
        // 아직 "로딩 중"인 페이지: 조금 뒤에 다시 (최대 2번), 그래도 비어 있으면 숨은 데이터로 읽은 만큼만
        if page.loading && page.text.chars().count() < 200 {
            let tries = {
                let mut t = self.loading_tries.lock().unwrap();
                let n = t.entry(raw.to_string()).or_insert(0);
                *n += 1;
                *n
            };
            if tries <= 2 {
                self.frontier.lock().unwrap().queue.push_back(raw.to_string());
                return Ok(Step::Skip);
            }
            self.note_problem(&host, "페이지가 '로딩 중'에서 바뀌지 않습니다 (자바스크립트로 화면을 그리는 사이트)");
        }
        // 다시 확인하러 온 페이지: 내용이 바뀌었으면 새 내용 저장
        if refresh {
            self.refreshed.lock().unwrap().insert(raw.to_string(), Instant::now());
            let h = {
                use std::hash::{Hash, Hasher};
                let mut x = std::collections::hash_map::DefaultHasher::new();
                page.text.hash(&mut x);
                x.finish()
            };
            let url = crawler::canonical(&final_url);
            let old = self.page_hash.lock().unwrap().insert(url.clone(), h);
            self.save_hashes();
            if old.is_some() && old != Some(h) && page.text.chars().count() >= 200 {
                self.save_text(&page.title, &url, "웹", &page.text);
                return Ok(Step::Learned(format!("{} (바뀐 내용)", page.title), host));
            }
            return Ok(Step::Skip);
        }

        let only_here = s.only.as_deref().map_or(false, |site| host_matches(host.split(':').next().unwrap_or(&host), site));
        if only_here && (!page.nofollow || !s.obey_robots) {
            // 한 사이트만 공부: 그 사이트 링크를 많이(최대 60개), 맨 앞에 넣음
            let mut items: Vec<String> = page
                .links
                .iter()
                .filter(|l| crawler::url_ok(l, &s.blocked_domains) && Self::host_of(l) == host)
                .map(crawler::canonical)
                .collect();
            items.dedup();
            items.truncate(60);
            if items.is_empty() && page.text.chars().count() < 50 {
                self.note_problem(&host, "페이지에서 내용·링크를 찾지 못했습니다 (자바스크립트로 화면을 그리는 사이트일 수 있습니다)");
            }
            self.push_targets_front(items);
        } else if !page.nofollow || !s.obey_robots {
            // 링크: 같은 사이트 4개 + 다른 사이트 10개 (한쪽으로 안 쏠리게, 여러 일꾼이 동시에 읽을 사이트가 많게)
            let (mut same, mut other) = (0, 0);
            let mut items = Vec::new();
            for l in page
                .links
                .iter()
                .filter(|l| crawler::url_ok(l, &s.blocked_domains))
            {
                let is_same = Self::host_of(l) == host;
                if (is_same && same < 4) || (!is_same && other < 10) {
                    if is_same {
                        same += 1
                    } else {
                        other += 1
                    }
                    items.push(crawler::canonical(l));
                }
            }
            self.push_targets(items);
        }
        let generic = ["home", "홈", "index", "welcome", "main page", "untitled"];
        if (page.noindex && s.obey_robots)
            || page.title.chars().count() < 2
            || generic.contains(&page.title.to_lowercase().as_str())
            || page.summary.chars().count() < 20
            || !crawler::content_ok(&page.title, &page.summary)
        {
            return Ok(Step::Skip);
        }
        if self
            .index
            .read()
            .unwrap()
            .contains_key(&normalize(&page.title))
        {
            return Ok(Step::Skip); // 이미 아는 페이지 (주소만 다른 같은 페이지 등)
        }
        let url = crawler::canonical(&final_url);
        self.save_text(&page.title, &url, "웹", &page.text);
        {
            use std::hash::{Hash, Hasher};
            let mut x = std::collections::hash_map::DefaultHasher::new();
            page.text.hash(&mut x);
            self.page_hash.lock().unwrap().insert(url.clone(), x.finish());
            self.save_hashes();
        }
        let doc = self.add(Doc {
            title: page.title,
            summary: page.summary,
            url,
            source: "웹".into(),
            learned_at: now(),
        });
        Ok(Step::Learned(doc.title, host))
    }

    // ───────────── 채팅에서 웹 탐색 (사용자가 요청한 것) ─────────────

    /// 사용자가 준 주소 읽기. 사용자가 직접 요청한 거라 robots.txt 대신 차단 목록·내용 필터만 적용
    pub fn read_page(&self, raw: &str, blocked: &[String]) -> Result<WebPage, String> {
        let raw = if raw.starts_with("http") {
            raw.to_string()
        } else {
            format!("https://{raw}")
        };
        let u = Url::parse(&raw).map_err(|_| "주소 형식이 올바르지 않습니다.".to_string())?;
        if !crawler::url_ok(&u, blocked) {
            return Err("안전하지 않거나 차단된 주소라서 열지 않겠습니다.".into());
        }
        let resp = match net::get(u.as_str(), &[]) {
            Ok(r) => r,
            Err(e) => return Err(format!("페이지를 열 수 없습니다. {}", net::describe(&e))),
        };
        if !resp.content_type().contains("html") {
            return Err(format!(
                "웹페이지가 아닌 파일입니다. ({})",
                resp.content_type()
            ));
        }
        let final_url = Url::parse(resp.get_url()).unwrap_or(u);
        let mut html = String::new();
        std::io::Read::read_to_string(
            &mut std::io::Read::take(resp.into_reader(), MAX_HTML),
            &mut html,
        )
        .map_err(|_| "페이지를 읽지 못했습니다.".to_string())?;
        let page = crawler::parse_html(&html, &final_url);
        if !crawler::content_ok(&page.title, &page.summary) {
            return Err("부적절한 내용이 있는 페이지라서 보여 드리지 않겠습니다.".into());
        }
        let mut links = Vec::new();
        for (l, t) in page.links.iter().zip(&page.link_texts) {
            if t.chars().count() >= 2
                && crawler::url_ok(l, blocked)
                && !links.iter().any(|(_, x): &(String, String)| x == t)
            {
                links.push((crawler::canonical(l), t.chars().take(40).collect()));
            }
            if links.len() >= 6 {
                break;
            }
        }
        let url = crawler::canonical(&final_url);
        if page.title.chars().count() >= 2 && page.summary.chars().count() >= 20 && !page.noindex {
            if !self.index.read().unwrap().contains_key(&normalize(&page.title)) {
                self.save_text(&page.title, &url, "웹", &page.text);
            }
            self.add(Doc {
                title: page.title.clone(),
                summary: page.summary.clone(),
                url: url.clone(),
                source: "웹".into(),
                learned_at: now(),
            });
        }
        Ok(WebPage {
            title: page.title,
            summary: page.summary,
            headings: page.headings,
            links,
            url,
            site: Self::host_of(&final_url),
        })
    }

    /// 채팅 검색. Brave API 키가 있으면 일반 웹, 없으면 위키백과(한·영) + DuckDuckGo 요약
    pub fn search(&self, q: &str, brave_key: &str) -> Result<Vec<SearchHit>, String> {
        let mut hits = Vec::new();
        if !brave_key.trim().is_empty() {
            let url = format!(
                "https://api.search.brave.com/res/v1/web/search?count=5&q={}",
                urlencoding::encode(q)
            );
            let v: serde_json::Value = net::get(
                &url,
                &[
                    ("Accept", "application/json"),
                    ("X-Subscription-Token", brave_key.trim()),
                ],
            )
            .map_err(|e| format!("Brave 검색 실패: {}", net::describe(&e)))?
            .into_json()
            .map_err(|e| e.to_string())?;
            for r in v["web"]["results"].as_array().into_iter().flatten().take(5) {
                hits.push(SearchHit {
                    title: strip_tags(r["title"].as_str().unwrap_or("")),
                    snippet: strip_tags(r["description"].as_str().unwrap_or("")),
                    url: r["url"].as_str().unwrap_or("").into(),
                    source: "Brave Search".into(),
                });
            }
            return Ok(hits);
        }
        // DuckDuckGo 공식 요약 API (키 필요 없음)
        let ddg = format!(
            "https://api.duckduckgo.com/?format=json&no_html=1&skip_disambig=1&q={}",
            urlencoding::encode(q)
        );
        if let Ok(v) = self.get_json(&ddg) {
            let abs = v["AbstractText"].as_str().unwrap_or("");
            if !abs.is_empty() {
                hits.push(SearchHit {
                    title: v["Heading"].as_str().unwrap_or(q).into(),
                    snippet: shorten(abs, 300),
                    url: v["AbstractURL"].as_str().unwrap_or("").into(),
                    source: format!(
                        "DuckDuckGo · {}",
                        v["AbstractSource"].as_str().unwrap_or("")
                    ),
                });
            }
        }
        for lang in ["ko", "en"] {
            let url = format!(
                "{}/w/api.php?action=query&list=search&srlimit=3&format=json&srsearch={}",
                Self::wiki(lang),
                urlencoding::encode(q)
            );
            let v = self.get_json(&url)?;
            for r in v["query"]["search"].as_array().into_iter().flatten() {
                let title = r["title"].as_str().unwrap_or("").to_string();
                if hits.iter().any(|h: &SearchHit| h.title == title) {
                    continue;
                }
                hits.push(SearchHit {
                    url: format!(
                        "{}/wiki/{}",
                        Self::wiki(lang),
                        urlencoding::encode(&title.replace(' ', "_"))
                    ),
                    snippet: strip_tags(r["snippet"].as_str().unwrap_or("")),
                    title,
                    source: format!("위키백과 ({lang})"),
                });
            }
            if hits.len() >= 4 {
                break;
            }
        }
        Ok(hits)
    }

    // ───────────── 질문 ─────────────

    /// 질문 등록. 새로 등록됐으면 true
    pub fn ask(&self, q: Question) -> bool {
        let mut qs = self.questions.lock().unwrap();
        if qs.contains(&q) || qs.len() >= QUESTIONS_MAX {
            return false;
        }
        qs.push_back(q);
        let _ = std::fs::write(
            self.dir.join("questions.json"),
            serde_json::to_string(&*qs).unwrap(),
        );
        true
    }

    pub fn current_question(&self) -> Option<Question> {
        self.questions.lock().unwrap().front().cloned()
    }

    pub fn pop_question(&self) {
        let mut qs = self.questions.lock().unwrap();
        qs.pop_front();
        let _ = std::fs::write(
            self.dir.join("questions.json"),
            serde_json::to_string(&*qs).unwrap(),
        );
    }

    /// 현재 질문에 대한 사용자 대답 처리. 결과 메시지를 돌려줌.
    pub fn answer(&self, ans: &str) -> Result<String, String> {
        let Some(q) = self.current_question() else {
            return Ok("지금은 여쭤볼 게 없습니다.".into());
        };
        let msg = match &q {
            Question::Reply { phrase } => {
                self.teach(phrase, ans);
                format!("알겠습니다. 앞으로 '{phrase}'라고 하시면 그렇게 대답하겠습니다.")
            }
            Question::Define { topic } => {
                self.add(Doc {
                    title: topic.clone(),
                    summary: ans.into(),
                    url: String::new(),
                    source: "사용자".into(),
                    learned_at: now(),
                });
                format!("'{topic}'에 대해 배웠습니다. 감사합니다.")
            }
            Question::Choose { options, .. } => {
                let pick = ans
                    .trim()
                    .trim_end_matches('번')
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| options.get(n.wrapping_sub(1)).cloned())
                    .unwrap_or_else(|| ans.trim().to_string());
                match self.learn(&pick, "질문")? {
                    Learned::Doc(d) => format!("'{}'을(를) 공부했습니다.\n{}", d.title, d.summary),
                    Learned::Ambiguous { title, .. } => {
                        format!("'{title}'도 뜻이 여러 개라 건너뛰겠습니다.")
                    }
                    Learned::NotFound => format!("'{pick}' 문서를 찾지 못했습니다."),
                }
            }
        };
        self.pop_question();
        Ok(msg)
    }

    // ───────────── 가르쳐 준 대답 ─────────────

    pub fn teach(&self, phrase: &str, reply: &str) {
        let (p, r) = (phrase.trim(), reply.trim());
        if p.is_empty() || r.is_empty() {
            return;
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("taught.jsonl"))
        {
            let _ = writeln!(
                f,
                "{}",
                serde_json::to_string(&Taught {
                    user: p.into(),
                    bot: r.into()
                })
                .unwrap()
            );
        }
        self.taught.write().unwrap().insert(phrase_key(p), r.into());
    }

    /// 가르쳐 준 말이면 그 대답 (띄어쓰기·문장부호 차이는 무시)
    pub fn taught_reply(&self, text: &str) -> Option<String> {
        let n = phrase_key(text);
        (!n.is_empty())
            .then(|| self.taught.read().unwrap().get(&n).cloned())
            .flatten()
    }

    // ───────────── 피드백 (3단계) ─────────────

    /// 👍/👎 저장. 👎 + 더 좋은 대답이면 바로 가르친 대답으로도 등록
    pub fn add_feedback(
        &self,
        user: &str,
        reply: &str,
        rating: i8,
        better: Option<&str>,
    ) -> String {
        let better = better
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(String::from);
        let f = Feedback {
            user: user.trim().into(),
            reply: reply.trim().into(),
            rating: rating.signum(),
            better: better.clone(),
            time: now(),
        };
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("feedback.jsonl"))
        {
            let _ = writeln!(file, "{}", serde_json::to_string(&f).unwrap());
        }
        self.feedback.write().unwrap().push(f);
        match (rating > 0, better) {
            (true, _) => "감사합니다. 이런 대답을 더 자주 하겠습니다.".into(),
            (false, Some(b)) => {
                self.teach(user, &b);
                "알겠습니다. 다음부터는 알려 주신 대로 대답하겠습니다.".into()
            }
            (false, None) => {
                self.ask(Question::Reply {
                    phrase: user.trim().into(),
                });
                "죄송합니다. 그 대답은 피하겠습니다. 학습 탭에서 더 좋은 대답을 알려 주시면 배우겠습니다.".into()
            }
        }
    }

    /// 보상 점수: 비슷한 말에 👍 받은 대답과 닮으면 +, 👎 받은 대답과 닮으면 −
    /// (RLHF의 "좋은 대답 판별기"를 아주 작게 만든 것. 모델 재학습 전에도 피드백이 바로 반영됨)
    pub fn reward(&self, user: &str, candidate: &str) -> f32 {
        let (u, c) = (phrase_key(user), phrase_key(candidate));
        let mut score = 0.0;
        for f in self.feedback.read().unwrap().iter() {
            let rel = bigram_sim(&u, &phrase_key(&f.user));
            if rel < 0.6 {
                continue;
            }
            let sim = bigram_sim(&c, &phrase_key(&f.reply));
            score += rel * sim * f.rating as f32;
            if f.rating < 0 && c == phrase_key(&f.reply) {
                score -= 10.0; // 👎 받은 대답 그대로는 다시 안 함
            }
        }
        score
    }

    /// 생각하기: 후보들을 여러 기준으로 점수 매겨서 제일 좋은 것 (best-of-N)
    ///   피드백 보상 + 자연스러움 + 길이(조금) − 최근 대답과 비슷함
    /// 후보 하나 채점 (피드백 보상 + 자연스러움 − 최근 대답과 겹침 + 길이)
    /// 돌려주는 값: (점수, 겹침 정도)
    pub fn judge(&self, user: &str, c: &str, fluency: f32, prefer_long: bool) -> (f32, f32) {
        let key = phrase_key(c);
        let repeat = self
            .recent
            .lock()
            .unwrap()
            .iter()
            .map(|r| bigram_sim(&key, &phrase_key(r)))
            .fold(0.0f32, f32::max);
        let len = c.chars().count() as f32;
        let score = self.reward(user, c) + 1.5 * fluency - 1.2 * repeat
            + if prefer_long { 0.25 * (len / 40.0).min(2.0) } else { 0.0 };
        (score, repeat)
    }

    /// 고른 대답을 "최근 대답"에 넣음 (다음엔 다른 대답이 나오게)
    pub fn remember_pick(&self, c: &str) {
        let mut r = self.recent.lock().unwrap();
        r.push_back(c.to_string());
        while r.len() > 12 {
            r.pop_front();
        }
    }

    pub fn choose(
        &self,
        user: &str,
        candidates: Vec<(String, f32)>,
        prefer_long: bool,
    ) -> Option<String> {
        let mut best: Option<(f32, String)> = None;
        for (c, fluency) in candidates.into_iter().filter(|(c, _)| c.trim().chars().count() >= 2) {
            let (score, _) = self.judge(user, &c, fluency, prefer_long);
            if best.as_ref().map_or(true, |(b, _)| score > *b) {
                best = Some((score, c));
            }
        }
        let pick = best.map(|(_, c)| c)?;
        self.remember_pick(&pick);
        Some(pick)
    }

    /// 후보 대답들 중 보상이 제일 높은 것 (best-of-N)
    pub fn pick_best(&self, user: &str, candidates: Vec<String>) -> Option<String> {
        let mut best: Option<(f32, String)> = None;
        for c in candidates.into_iter().filter(|c| !c.trim().is_empty()) {
            let r = self.reward(user, &c);
            if best.as_ref().map_or(true, |(b, _)| r > *b) {
                best = Some((r, c));
            }
        }
        best.map(|(_, c)| c)
    }

    // ───────────── 스스로 공부 ─────────────

    /// 한 걸음: 대기열에서 하나 꺼내 공부
    pub fn study_step(&self, s: &CrawlSettings) -> Result<Studied, String> {
        {
            let mut t = self.today.lock().unwrap();
            let d = chrono::Local::now().format("%Y-%m-%d").to_string();
            if t.0 != d {
                *t = (d, 0);
            }
            if t.1 >= DAILY_LIMIT {
                return Ok(Studied::Idle);
            }
        }
        let mut postponed = 0;
        for _ in 0..40 {
            let next = {
                let mut f = self.frontier.lock().unwrap();
                match &s.only {
                    // 한 사이트만: 대기열에서 그 사이트 것만 꺼냄 (나머지는 나중을 위해 그대로 둠)
                    Some(site) => f.queue.iter().position(|it| host_matches(&item_host(it), site)).and_then(|i| f.queue.remove(i)),
                    None => f.queue.pop_front(),
                }
            };
            let Some(item) = next else {
                // 위키백과에서만 공부하는데 읽을 문서가 떨어졌으면 아무 문서나 더 가져옴
                if postponed == 0 && s.only.as_deref() == Some("wikipedia.org") && !s.wiki_langs.is_empty() {
                    let mut added = 0;
                    for lang in &s.wiki_langs {
                        let items = self.random_wiki(lang, 10);
                        added += items.len();
                        self.frontier.lock().unwrap().queue.extend(items);
                    }
                    if added > 0 {
                        continue;
                    }
                }
                // 다른 사이트만 공부할 때: 사이트맵 → 다시 확인
                if postponed == 0 {
                    if let Some(site) = s.only.as_deref().filter(|x| *x != "wikipedia.org") {
                        if self.refill_site(site, s) > 0 {
                            continue;
                        }
                    }
                }
                return Ok(if postponed > 0 {
                    Studied::Wait
                } else {
                    Studied::Idle
                });
            };
            let step = match target(&item) {
                Target::Wiki(lang, title) => {
                    if !s.wiki_langs.iter().any(|l| *l == lang)
                        || self.index.read().unwrap().contains_key(&normalize(&title))
                    {
                        continue;
                    }
                    {
                        let mut n = self.wiki_inflight.lock().unwrap();
                        if *n >= 3 {
                            drop(n);
                            self.frontier.lock().unwrap().queue.push_back(item);
                            postponed += 1;
                            continue;
                        }
                        *n += 1;
                    }
                    let res = self.learn_wiki(&lang, &title, "탐색", Some(s));
                    *self.wiki_inflight.lock().unwrap() -= 1;
                    match res {
                        Ok(Learned::Doc(d)) => {
                            Ok(Step::Learned(d.title, format!("{lang}.wikipedia.org")))
                        }
                        Ok(Learned::Ambiguous { title, options }) => {
                            if lang == "ko" && !options.is_empty() {
                                self.ask(Question::Choose { title, options });
                            }
                            Ok(Step::Skip)
                        }
                        Ok(Learned::NotFound) => Ok(Step::Skip),
                        Err(e) => Err(e),
                    }
                }
                Target::Web(url) => {
                    if !s.explore_web {
                        continue;
                    }
                    self.crawl(&url, s, false)
                }
                Target::Refresh(url) => self.crawl(&url, s, true),
            };
            match step {
                Ok(Step::Learned(title, site)) => {
                    self.today.lock().unwrap().1 += 1;
                    return Ok(Studied::Learned { title, site });
                }
                Ok(Step::Skip) => {}
                Ok(Step::Later) => {
                    postponed += 1;
                    self.frontier.lock().unwrap().queue.push_back(item);
                }
                Err(e) => {
                    self.frontier.lock().unwrap().queue.push_front(item);
                    return Err(e);
                }
            }
        }
        Ok(Studied::Wait)
    }

    /// 백그라운드 스레드. 설정의 self_study 가 켜져 있을 때만 공부함 (/Start, /Stop)
    /// 일꾼 여러 명이 동시에 서로 다른 사이트를 읽음 (같은 사이트는 간격을 지키며 한 명씩).
    /// 일꾼 수는 설정의 study_workers (기본 8). 0번 일꾼만 알림·질문을 띄움.
    pub fn start_explorer(self: &Arc<Self>, config: Arc<RwLock<Config>>, events: EventSink) {
        let n = config.read().unwrap().study_workers.clamp(1, 32);
        for id in 0..n {
            let me = self.clone();
            let config = config.clone();
            let events = events.clone();
            std::thread::spawn(move || me.explore_loop(id, config, events));
        }
    }

    fn explore_loop(self: Arc<Self>, id: usize, config: Arc<RwLock<Config>>, events: EventSink) {
        let me = self;
        {
            std::thread::sleep(Duration::from_secs(3 + id as u64 / 4));
            let mut last_q: Option<Question> = None;
            loop {
                let (on, interval, settings) = {
                    let c = config.read().unwrap();
                    (c.self_study, c.study_interval_secs, CrawlSettings::from(&c))
                };
                if !on {
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
                let wait = match me.study_step(&settings) {
                    Ok(Studied::Learned { title, site }) => {
                        events(Event::Learned {
                            title,
                            site,
                            count: me.stats().count,
                        });
                        interval
                    }
                    Ok(Studied::Wait) => 0, // 아래에서 0.2초만 쉼
                    Ok(Studied::Idle) if id != 0 => 30, // 다른 일꾼은 조용히 잠깐 쉼
                    Err(_) if id != 0 => 60,
                    Ok(Studied::Idle) => {
                        events(Event::StudyLog {
                            text: match &settings.only {
                                Some(site) => match me.site_problem(site) {
                                    Some(why) => format!("{site} 에서 더 읽을 곳을 찾지 못했습니다. 이유: {why}. 10분 뒤 다시 보겠습니다. 다른 링크로 /Start only 를 하시거나 /Start 로 어디서든 공부하게 할 수 있습니다."),
                                    None => format!("{site} 에서 더 읽을 곳을 찾지 못했습니다. (읽을 만한 새 페이지가 없음) 10분 뒤 다시 보겠습니다. 다른 링크로 /Start only 를 하시거나 /Start 로 어디서든 공부하게 할 수 있습니다."),
                                },
                                None => "읽을 곳이 없거나 오늘 공부량을 다 채웠습니다. 10분 뒤 다시 시작하겠습니다.".into(),
                            },
                        });
                        600
                    }
                    Err(e) => {
                        events(Event::StudyLog {
                            text: format!("공부를 잠시 멈춥니다. {e} (5분 뒤 다시 시도)"),
                        });
                        300
                    }
                };
                let q = if id == 0 { me.current_question() } else { None };
                if q.is_some() && q != last_q {
                    events(Event::Question {
                        text: q.as_ref().unwrap().text(),
                    });
                }
                last_q = q;
                let woke = me.wake_count();
                if wait == 0 {
                    std::thread::sleep(Duration::from_millis(200));
                }
                for _ in 0..wait * 10 {
                    // 멈추거나, 새로 /Start 하면 기다리지 않고 바로
                    if !config.read().unwrap().self_study || me.wake_count() != woke {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_site_helpers() {
        assert_eq!(site_of("https://ko.wikipedia.org/wiki/블랙홀").as_deref(), Some("wikipedia.org"));
        assert_eq!(site_of("https://www.namu.wiki/w/X").as_deref(), Some("namu.wiki"));
        assert_eq!(site_of("https://docs.python.org/3/").as_deref(), Some("docs.python.org"));
        assert_eq!(wiki_item_of("https://en.wikipedia.org/wiki/Black_hole").as_deref(), Some("wiki:en:Black hole"));
        assert_eq!(wiki_item_of("https://ko.wikipedia.org/wiki/%EB%B8%94%EB%9E%99%ED%99%80").as_deref(), Some("블랙홀"));
        assert!(host_matches(&item_host("블랙홀"), "wikipedia.org"));
        assert!(host_matches(&item_host("wiki:ja:科学"), "wikipedia.org"));
        assert!(!host_matches(&item_host("https://namu.wiki/w/x"), "wikipedia.org"));
        assert!(host_matches("www.namu.wiki", "namu.wiki"));
        assert!(!host_matches("evilnamu.wiki", "namu.wiki"));
    }

    use super::*;

    #[test]
    fn helpers() {
        assert!(bigram_sim("상대성이론", &normalize("상대성 이론")) > 0.99);
        assert!(bigram_sim("블랙홀", "화이트홀") < 0.8);
        assert!(!worth_studying("1999년"));
        assert!(!worth_studying("3월 1일"));
        assert!(!worth_studying("한국의 산 목록"));
        assert!(worth_studying("블랙홀"));
        assert!(worth_studying("GPT-4"));
        assert_eq!(
            shorten("첫 문장입니다. 둘째 문장입니다. 셋째 문장입니다.", 20),
            "첫 문장입니다. 둘째 문장입니다."
        );
        assert_eq!(normalize("배고파!!"), normalize("배 고파"));
        assert_eq!(phrase_key("뭐함 ㅋㅋ"), phrase_key("뭐함?"));
        assert_eq!(phrase_key("ㅎㅇ"), "ㅎㅇ");
    }

    #[test]
    fn store_questions_teaching() {
        let dir = std::env::temp_dir().join(format!("arcana-kn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let k = Knowledge::open(dir.clone());
        assert_eq!(k.stats().queued, SEEDS.len() + SEEDS_WORLD.len());

        k.add(Doc {
            title: "상대성 이론".into(),
            summary: "…".into(),
            url: "u".into(),
            source: "질문".into(),
            learned_at: "-".into(),
        });
        assert!(k.lookup("상대성이론").is_some());
        assert!(k.lookup("양자역학").is_none());

        k.ask(Question::Reply {
            phrase: "뭐함 ㅋ".into(),
        });
        k.ask(Question::Reply {
            phrase: "뭐함 ㅋ".into(),
        }); // 중복은 무시
        k.ask(Question::Define {
            topic: "얼로니".into(),
        });
        assert_eq!(k.stats().questions, 2);
        assert!(k.answer("그냥 대기 중입니다.").unwrap().contains("앞으로"));
        assert_eq!(
            k.taught_reply("뭐함").as_deref(),
            Some("그냥 대기 중입니다.")
        );
        assert!(k
            .answer("Arcana를 만든 사람")
            .unwrap()
            .contains("배웠습니다"));
        assert!(k.lookup("얼로니").is_some());
        assert!(k.current_question().is_none());

        // 피드백: 👎 받은 대답은 안 고르고, 👍 받은 쪽을 고름
        k.add_feedback("심심해", "TSB 한 판 어떠십니까?", -1, None);
        k.add_feedback("심심해", "유튜브에서 재미있는 영상 찾아 드릴까요?", 1, None);
        let pick = k.pick_best(
            "심심해 ㅋㅋ",
            vec![
                "TSB 한 판 어떠십니까?".into(),
                "유튜브 영상 찾아 드릴까요?".into(),
            ],
        );
        assert_eq!(pick.as_deref(), Some("유튜브 영상 찾아 드릴까요?"));
        k.add_feedback("뭐함", "네.", -1, Some("대기하고 있습니다."));
        assert_eq!(
            k.taught_reply("뭐함").as_deref(),
            Some("대기하고 있습니다.")
        );
        drop(k);

        let k2 = Knowledge::open(dir.clone()); // 다시 열어도 남아있는지
        assert_eq!(k2.stats().count, 2);
        assert_eq!(k2.stats().taught, 1);
        assert_eq!((k2.stats().likes, k2.stats().dislikes), (1, 2));
        let _ = std::fs::remove_dir_all(dir);
    }
}

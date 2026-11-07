//! 대화 맥락 기억 + 컨테이너 메모리 (대화 기록)
//!
//! "디코 켜줘" → "그거 꺼줘"           : 그거 = 디스코드
//! "파이썬 정렬 검색해줘" → "아까 검색한 거 다시" : 같은 검색
//! "블랙홀이 뭐야" → "그거 더 알려줘"      : 블랙홀
//! "라면 3분 타이머" → "한 번 더"          : 같은 명령 다시
//!
//! 대화 기록은 %APPDATA%\Mayhem\history.jsonl 에 저장 (컨테이너 메모리).
//! 설정한 크기(10MB ~ 10GB)를 넘으면 오래된 것부터 지움. 앱을 다시 켜도 맥락이 이어짐.

use arcana_core::Understanding;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub time: String,
    pub user: String,
    pub reply: String,
    pub intent: String,
    #[serde(default)]
    pub slots: BTreeMap<String, String>,
}

/// 지금 대화에서 "그거"가 가리킬 수 있는 것들
#[derive(Debug, Clone, Default, Serialize)]
pub struct State {
    pub last_app: Option<String>,
    pub last_query: Option<String>,
    pub last_topic: Option<String>,
    pub last_url: Option<String>,
    /// 다시 할 수 있는 마지막 명령 (의도, 슬롯)
    pub last_command: Option<(String, BTreeMap<String, String>)>,
}

pub struct Container {
    path: PathBuf,
    recent: Mutex<VecDeque<Turn>>,
    state: Mutex<State>,
}

const RECENT: usize = 50;
/// 다시 하면 안 되는 명령 (확인이 필요하거나 되돌릴 수 없는 것)
const NO_REPEAT: &[&str] = &["shutdown", "memo_clear", "lock"];

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

fn has_any(t: &str, words: &[&str]) -> bool {
    words.iter().any(|w| t.contains(w))
}

const POINTERS: &[&str] = &["그거", "그것", "그걸", "그게", "걔", "그앱", "그프로그램", "아까거", "방금거", "아까그", "방금그", "거기", "그사이트", "그페이지", "그링크", "그주제", "그얘기"];
const REPEAT_WORDS: &[&str] = &["다시", "또", "한번더", "아까", "방금"];
const REPEAT: &[&str] = &["다시", "다시해줘", "다시해", "한번더", "한번더해줘", "또해줘", "똑같이해줘", "방금거다시", "아까거다시"];

impl Container {
    pub fn open(dir: &std::path::Path) -> Self {
        let path = dir.join("history.jsonl");
        let mut recent = VecDeque::new();
        if let Ok(s) = std::fs::read_to_string(&path) {
            let lines: Vec<&str> = s.lines().collect();
            for l in lines.iter().skip(lines.len().saturating_sub(RECENT)) {
                if let Ok(t) = serde_json::from_str::<Turn>(l) {
                    recent.push_back(t);
                }
            }
        }
        let c = Self { path, recent: Mutex::new(VecDeque::new()), state: Mutex::new(State::default()) };
        for t in recent {
            c.update_state(&t.intent, &t.slots);
            c.recent.lock().unwrap().push_back(t);
        }
        c
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    pub fn recent(&self, n: usize) -> Vec<Turn> {
        let r = self.recent.lock().unwrap();
        r.iter().skip(r.len().saturating_sub(n)).cloned().collect()
    }

    /// "그거", "아까", "다시" 같은 말을 실제 대상으로 바꿈. 바꿨으면 true
    pub fn resolve(&self, text: &str, u: &mut Understanding) -> bool {
        let t = norm(text);
        let st = self.state();
        // 1) "다시", "한 번 더" → 마지막 명령 그대로
        if REPEAT.contains(&t.as_str()) {
            if let Some((intent, slots)) = st.last_command.clone() {
                if !NO_REPEAT.contains(&intent.as_str()) {
                    *u = Understanding { intent, confidence: 1.0, slots };
                    return true;
                }
            }
            return false;
        }
        // "다시 켜줘", "또 열어줘" 처럼 대상 없이 다시 하라는 말도 마지막 대상을 가리킴
        let again = (t.starts_with("다시") || t.starts_with("또") || t.starts_with("한번더"))
            && !u.slots.values().any(|v| !REPEAT_WORDS.contains(&norm(v).as_str()) && !POINTERS.contains(&norm(v).as_str()));
        let pointer = has_any(&t, POINTERS) || t.starts_with("아까") || t.starts_with("방금") || again;
        if !pointer {
            return false;
        }
        let set = |u: &mut Understanding, intent: &str, key: &str, val: &Option<String>| -> bool {
            match val {
                Some(v) => {
                    u.intent = intent.into();
                    u.confidence = u.confidence.max(0.9);
                    u.slots.insert(key.into(), v.clone());
                    true
                }
                None => false,
            }
        };
        // 2) 무슨 동작인지는 문장의 동사로
        if has_any(&t, &["사이트", "페이지", "링크", "주소"]) && st.last_url.is_some() {
            return set(u, "read_page", "URL", &st.last_url);
        }
        if has_any(&t, &["검색", "찾아", "구글"]) {
            let q = st.last_query.clone().or(st.last_topic.clone());
            return set(u, "web_search", "QUERY", &q);
        }
        if has_any(&t, &["유튜브", "틀어", "재생"]) {
            return set(u, "youtube", "QUERY", &st.last_query);
        }
        if has_any(&t, &["꺼", "끄", "닫", "종료"]) {
            return set(u, "close_app", "APP", &st.last_app);
        }
        if has_any(&t, &["켜", "열어", "실행", "띄워"]) {
            return set(u, "open_app", "APP", &st.last_app);
        }
        if has_any(&t, &["뭐야", "알려", "설명", "자세히", "더", "누구", "어디"]) {
            return set(u, "ask_knowledge", "TOPIC", &st.last_topic);
        }
        false
    }

    fn update_state(&self, intent: &str, slots: &BTreeMap<String, String>) {
        let mut st = self.state.lock().unwrap();
        let get = |k: &str| slots.get(k).filter(|v| !v.trim().is_empty()).cloned();
        match intent {
            "open_app" | "close_app" => st.last_app = get("APP").or(st.last_app.take()),
            "web_search" | "youtube" => st.last_query = get("QUERY").or(st.last_query.take()),
            "ask_knowledge" => st.last_topic = get("TOPIC").or(st.last_topic.take()),
            "read_page" => st.last_url = get("URL").or(st.last_url.take()),
            _ => {}
        }
        let actionable = !matches!(intent, "unknown" | "taught" | "greet" | "thanks" | "bye" | "who_are_you" | "memory");
        if actionable {
            st.last_command = Some((intent.to_string(), slots.clone()));
        }
    }

    /// 한 턴 기록 (enabled = 컨테이너 메모리 켜짐, max_mb = 최대 크기)
    pub fn record(&self, user: &str, reply: &str, intent: &str, slots: &BTreeMap<String, String>, enabled: bool, max_mb: u64) {
        self.update_state(intent, slots);
        let turn = Turn {
            time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            user: user.into(),
            reply: reply.into(),
            intent: intent.into(),
            slots: slots.clone(),
        };
        {
            let mut r = self.recent.lock().unwrap();
            r.push_back(turn.clone());
            while r.len() > RECENT {
                r.pop_front();
            }
        }
        if !enabled {
            return;
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{}", serde_json::to_string(&turn).unwrap());
        }
        self.trim(max_mb);
    }

    /// 최대 크기를 넘으면 오래된 기록부터 지워서 절반 정도로 줄임
    fn trim(&self, max_mb: u64) {
        let max = max_mb.clamp(10, 10 * 1024) * 1024 * 1024;
        let Ok(meta) = std::fs::metadata(&self.path) else { return };
        if meta.len() <= max {
            return;
        }
        if let Ok(s) = std::fs::read_to_string(&self.path) {
            let keep = (max / 2) as usize;
            let start = s.len().saturating_sub(keep);
            let start = s[start..].find('\n').map(|i| start + i + 1).unwrap_or(start);
            let _ = std::fs::write(&self.path, &s[start..]);
        }
    }

    pub fn usage_bytes(&self) -> u64 {
        std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_file(&self.path);
        self.recent.lock().unwrap().clear();
        *self.state.lock().unwrap() = State::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(intent: &str) -> Understanding {
        Understanding { intent: intent.into(), confidence: 0.9, slots: BTreeMap::new() }
    }

    fn slots(k: &str, v: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(k.to_string(), v.to_string())])
    }

    #[test]
    fn pointers_and_repeat() {
        let dir = std::env::temp_dir().join(format!("mayhem-ctx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let c = Container::open(&dir);
        c.record("디코 켜줘", "디스코드를 실행합니다.", "open_app", &slots("APP", "디코"), true, 10);
        let mut x = u("close_app");
        x.slots.insert("APP".into(), "그거".into());
        assert!(c.resolve("그거 꺼줘", &mut x));
        assert_eq!((x.intent.as_str(), x.slots["APP"].as_str()), ("close_app", "디코"));

        c.record("파이썬 정렬 검색해줘", "…", "web_search", &slots("QUERY", "파이썬 정렬"), true, 10);
        let mut y = u("unknown");
        assert!(c.resolve("아까 검색한 거 다시 찾아줘", &mut y));
        assert_eq!(y.slots["QUERY"], "파이썬 정렬");

        c.record("블랙홀이 뭐야", "…", "ask_knowledge", &slots("TOPIC", "블랙홀"), true, 10);
        let mut z = u("unknown");
        assert!(c.resolve("그거 더 자세히 알려줘", &mut z));
        assert_eq!((z.intent.as_str(), z.slots["TOPIC"].as_str()), ("ask_knowledge", "블랙홀"));

        c.record("라면 3분 타이머", "…", "set_timer", &slots("TIME", "3분"), true, 10);
        let mut w = u("unknown");
        assert!(c.resolve("한 번 더", &mut w));
        assert_eq!((w.intent.as_str(), w.slots["TIME"].as_str()), ("set_timer", "3분"));

        c.record("메모장 꺼줘", "…", "close_app", &slots("APP", "메모장"), true, 10);
        let mut again = u("open_app");
        again.slots.insert("APP".into(), "다시".into());
        assert!(c.resolve("다시 켜줘", &mut again));
        assert_eq!(again.slots["APP"], "메모장");
        let mut other = u("open_app");
        other.slots.insert("APP".into(), "크롬".into());
        assert!(!c.resolve("다시 크롬 켜줘", &mut other));

        let mut plain = u("unknown");
        assert!(!c.resolve("오늘 날씨 좋다", &mut plain));

        // 다시 열어도 맥락 유지
        let c2 = Container::open(&dir);
        assert_eq!(c2.state().last_app.as_deref(), Some("메모장"));
        assert!(c2.usage_bytes() > 0);
        c2.clear();
        assert_eq!(c2.usage_bytes(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}

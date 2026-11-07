//! 메모리: 사용자에 대해 오래 기억하는 것 (다른 AI들의 메모리와 같은 개념)
//!
//! - 대화에서 자동으로: "내 이름은 얼로니야", "내 생일은 3월 2일이야", "나는 민트초코 좋아해"
//! - 직접: "기억해: 나는 매운 거 못 먹어" 또는 설정 → 메모리에서 추가·삭제
//! - 물어보면 대답: "내 이름 기억해?", "내가 뭐 좋아한다고 했지?", "나에 대해 뭐 알아?"
//! 저장 위치: %APPDATA%\Mayhem\memory.json (이 PC에만 저장, 어디에도 안 보냄)

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fact {
    pub id: u64,
    /// name / birthday / like / dislike / note
    pub kind: String,
    pub value: String,
    pub created: String,
}

impl Fact {
    pub fn describe(&self) -> String {
        match self.kind.as_str() {
            "name" => format!("이름: {}", self.value),
            "birthday" => format!("생일: {}", self.value),
            "like" => format!("좋아하는 것: {}", self.value),
            "dislike" => format!("싫어하는 것: {}", self.value),
            _ => self.value.clone(),
        }
    }
}

pub struct Memory {
    path: PathBuf,
    facts: RwLock<Vec<Fact>>,
}

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap()
}

struct Patterns {
    name: Vec<Regex>,
    birthday: Regex,
    like: Regex,
    dislike: Regex,
    explicit: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        name: vec![
            re(r"(?:내|제|나의)\s*이름은\s*([가-힣A-Za-z0-9]{1,12}?)\s*(?:이야|야|입니다|이에요|예요|이다|임|이라고\s*해|라고\s*해)?[.!~]*$"),
            re(r"(?:나를|날|저를|절)\s*([가-힣A-Za-z0-9]{1,12}?)\s*(?:이라고|라고)\s*불러"),
        ],
        birthday: re(r"(?:내|제)\s*생일은\s*(.{2,20}?)\s*(?:이야|야|입니다|이에요|예요|임)?[.!~]*$"),
        like: re(r"^(?:나|난|저|전)(?:는|은)?\s*(.{1,20}?)\s*(?:을|를|이|가)?\s*(?:정말\s*|진짜\s*|제일\s*|엄청\s*)?좋아(?:해|함|합니다|해요)[.!~]*$"),
        dislike: re(r"^(?:나|난|저|전)(?:는|은)?\s*(.{1,20}?)\s*(?:을|를|이|가)?\s*(?:정말\s*|진짜\s*)?(?:싫어(?:해|함|합니다|해요)|못\s*먹어|못\s*먹습니다)[.!~]*$"),
        explicit: re(r"^(?:이거\s*)?기억해(?:줘|\s*줘|\s*둬|\s*두세요)?\s*[:：]\s*(.{2,200})$"),
    })
}

/// 받침에 맞는 조사 ("민트초코를", "라면을")
fn josa(w: &str, with: &str, without: &str) -> String {
    let has = w.chars().last().map(|c| ('가'..='힣').contains(&c) && (c as u32 - 0xAC00) % 28 != 0);
    match has {
        Some(true) => format!("{w}{with}"),
        Some(false) if w.chars().last().map_or(false, |c| ('가'..='힣').contains(&c)) => format!("{w}{without}"),
        _ => format!("{w}{with}({without})"),
    }
}

fn clean(s: &str) -> String {
    s.trim().trim_end_matches(['.', '!', '?', '~']).trim().to_string()
}

impl Memory {
    pub fn open(dir: &std::path::Path) -> Self {
        let path = dir.join("memory.json");
        let facts = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { path, facts: RwLock::new(facts) }
    }

    fn save(&self) {
        let f = self.facts.read().unwrap();
        let _ = std::fs::write(&self.path, serde_json::to_string_pretty(&*f).unwrap());
    }

    pub fn list(&self) -> Vec<Fact> {
        self.facts.read().unwrap().clone()
    }

    pub fn name(&self) -> Option<String> {
        self.facts.read().unwrap().iter().rev().find(|f| f.kind == "name").map(|f| f.value.clone())
    }

    pub fn add(&self, kind: &str, value: &str) -> Fact {
        let mut facts = self.facts.write().unwrap();
        // 이름·생일은 하나만 (새로 말하면 바뀜)
        if kind == "name" || kind == "birthday" {
            facts.retain(|f| f.kind != kind);
        }
        facts.retain(|f| !(f.kind == kind && f.value == value));
        let id = facts.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        let fact = Fact { id, kind: kind.into(), value: value.into(), created: chrono::Local::now().format("%Y-%m-%d").to_string() };
        facts.push(fact.clone());
        drop(facts);
        self.save();
        fact
    }

    pub fn remove(&self, id: u64) {
        self.facts.write().unwrap().retain(|f| f.id != id);
        self.save();
    }

    pub fn clear(&self) {
        self.facts.write().unwrap().clear();
        self.save();
    }

    /// 사용자 말에서 기억할 걸 찾아 저장. 저장했으면 대답 문장
    pub fn observe(&self, text: &str) -> Option<String> {
        let p = patterns();
        let t = text.trim();
        if let Some(c) = p.explicit.captures(t) {
            let v = clean(&c[1]);
            self.add("note", &v);
            return Some(format!("기억해 두겠습니다: {v}"));
        }
        for r in &p.name {
            if let Some(c) = r.captures(t) {
                let v = clean(&c[1]);
                self.add("name", &v);
                return Some(format!("반갑습니다, {v}님. 앞으로 {v}님이라고 부르겠습니다."));
            }
        }
        if let Some(c) = p.birthday.captures(t) {
            let v = clean(&c[1]);
            self.add("birthday", &v);
            return Some(format!("생일이 {v}이시군요. 기억해 두겠습니다."));
        }
        if let Some(c) = p.dislike.captures(t) {
            let v = clean(&c[1]);
            self.add("dislike", &v);
            return Some(format!("{} 피하시는군요. 기억해 두겠습니다.", josa(&v, "은", "는")));
        }
        if let Some(c) = p.like.captures(t) {
            let v = clean(&c[1]);
            self.add("like", &v);
            return Some(format!("{} 좋아하시는군요. 기억해 두겠습니다.", josa(&v, "을", "를")));
        }
        None
    }

    /// 사용자에 대한 질문이면 기억으로 대답
    pub fn answer(&self, text: &str) -> Option<String> {
        let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let facts = self.list();
        let of = |k: &str| facts.iter().filter(|f| f.kind == k).map(|f| f.value.clone()).collect::<Vec<_>>();
        let asks = |words: &[&str]| words.iter().any(|w| t.contains(w));
        let question = asks(&["뭐", "알아", "기억", "언제", "누구", "?", "했지", "했더라", "말해", "알려"]);
        if !question {
            return None;
        }
        if asks(&["내이름", "제이름", "나누구"]) {
            return Some(match self.name() {
                Some(n) => format!("{n}님이십니다."),
                None => "아직 이름을 모릅니다. \"내 이름은 ○○야\"라고 알려 주시면 기억하겠습니다.".into(),
            });
        }
        if asks(&["내생일", "제생일"]) {
            let b = of("birthday");
            return Some(match b.last() {
                Some(b) => format!("생일은 {b}입니다."),
                None => "아직 생일을 모릅니다.".into(),
            });
        }
        if asks(&["좋아한다고", "좋아하는거", "좋아하는것", "뭐좋아"]) && asks(&["내가", "제가", "나"]) {
            let l = of("like");
            return Some(if l.is_empty() { "아직 좋아하시는 걸 모릅니다.".into() } else { format!("{} 좋아하신다고 하셨습니다.", josa(&l.join(", "), "을", "를")) });
        }
        if asks(&["싫어한다고", "싫어하는거", "못먹는"]) && asks(&["내가", "제가", "나"]) {
            let d = of("dislike");
            return Some(if d.is_empty() { "아직 싫어하시는 걸 모릅니다.".into() } else { format!("{} 피하신다고 하셨습니다.", josa(&d.join(", "), "은", "는")) });
        }
        if asks(&["나에대해", "저에대해", "나에관해", "기억하는거", "기억하는것"]) {
            return Some(if facts.is_empty() {
                "아직 기억하고 있는 게 없습니다.".into()
            } else {
                format!("기억하고 있는 것들입니다.\n{}", facts.iter().map(|f| format!("- {}", f.describe())).collect::<Vec<_>>().join("\n"))
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_and_answer() {
        let dir = std::env::temp_dir().join(format!("mayhem-mem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let m = Memory::open(&dir);
        assert!(m.answer("내 이름 기억해?").unwrap().contains("모릅니다"));
        assert!(m.observe("내 이름은 얼로니야").is_some());
        assert_eq!(m.name().as_deref(), Some("얼로니"));
        assert!(m.observe("날 로니라고 불러줘").is_some());
        assert_eq!(m.name().as_deref(), Some("로니"));
        assert!(m.observe("내 생일은 3월 2일이야").is_some());
        assert!(m.observe("나는 민트초코 좋아해").is_some());
        assert!(m.observe("난 오이 못 먹어").is_some());
        assert!(m.observe("기억해: 매주 토요일엔 학원").is_some());
        assert_eq!(josa("민트초코", "을", "를"), "민트초코를");
        assert_eq!(josa("라면", "을", "를"), "라면을");
        assert!(m.observe("오늘 날씨 좋다").is_none());
        assert!(m.observe("라면 3분 타이머").is_none());
        assert_eq!(m.answer("내 이름 뭐야?").as_deref(), Some("로니님이십니다."));
        assert!(m.answer("내 생일 언제야").unwrap().contains("3월 2일"));
        assert!(m.answer("내가 뭐 좋아한다고 했지?").unwrap().contains("민트초코"));
        assert!(m.answer("나에 대해 뭐 알아?").unwrap().contains("토요일"));
        assert!(m.answer("블랙홀이 뭐야").is_none());
        let n = m.list().len();
        let id = m.list()[0].id;
        m.remove(id);
        assert_eq!(Memory::open(&dir).list().len(), n - 1); // 저장 확인
        m.clear();
        assert!(Memory::open(&dir).list().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}

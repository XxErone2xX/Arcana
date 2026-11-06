//! # arcana-skills — Arcana의 손발
//!
//! 두뇌(arcana-core)가 "이건 set_timer 의도야"라고 알려주면, 여기서 그 의도에 맞는
//! 스킬을 찾아 실제로 실행한다.
//!
//! ## 새 스킬 추가하는 법
//! 1. `src/skills/` 에 파일 하나 만들고 [`Skill`] 트레이트 구현
//! 2. `skills/mod.rs` 의 `builtin()` 목록에 추가
//! 3. 새 의도면 `training/data.py` 에 문장 템플릿 추가 후 재학습

pub mod config;
pub mod skills;
pub mod util;

use arcana_core::{Brain, Talker, Understanding};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub use config::Config;

/// 스킬이 앱(UI)에게 보내는 알림. UI 쪽에서 받아서 화면에 보여줌.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// 나중에 갑자기 알려야 할 때 (타이머 등)
    Alert { title: String, body: String },
    /// 그냥 대화창에 말 한 줄 추가
    Say { text: String },
}

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

/// 스킬이 실행될 때 받는 정보
pub struct Context {
    /// 사용자가 입력한 원문
    pub text: String,
    pub config: Arc<RwLock<Config>>,
    /// %APPDATA%\Arcana  (메모 등 저장)
    pub data_dir: PathBuf,
    pub events: EventSink,
    /// Arcana 대화 모델 (명령이 아닌 말에 대답)
    pub talker: Option<Arc<dyn Talker>>,
}

impl Context {
    pub fn emit(&self, e: Event) {
        (self.events)(e)
    }
    pub fn cfg(&self) -> std::sync::RwLockReadGuard<'_, Config> {
        self.config.read().unwrap()
    }
}

/// 스킬의 대답
#[derive(Debug, Clone, Serialize, Default)]
pub struct Reply {
    pub text: String,
    /// Some이면 UI가 "예/아니오" 버튼을 띄우고, 예를 누르면 [`Skill::confirmed`]가 호출됨
    pub confirm: Option<String>,
    /// true면 창 닫기
    pub quit: bool,
    /// 실패했는지 (UI에서 색 다르게 표시)
    pub error: bool,
}

impl Reply {
    pub fn say(t: impl Into<String>) -> Self {
        Self {
            text: t.into(),
            ..Default::default()
        }
    }
    pub fn error(t: impl Into<String>) -> Self {
        Self {
            text: t.into(),
            error: true,
            ..Default::default()
        }
    }
    pub fn ask(t: impl Into<String>) -> Self {
        let t = t.into();
        Self {
            confirm: Some(t.clone()),
            text: t,
            ..Default::default()
        }
    }
}

pub trait Skill: Send + Sync {
    /// 이 스킬이 담당하는 의도 이름들 (training/data.py 의 INTENTS 키와 같아야 함)
    fn intents(&self) -> &[&'static str];
    /// UI의 기능 목록에 보여줄 설명
    fn describe(&self) -> &'static str;
    fn run(&self, intent: &str, u: &Understanding, cx: &Context) -> Reply;
    /// [`Reply::ask`]로 물어본 뒤 사용자가 "예"를 눌렀을 때
    fn confirmed(&self, _intent: &str, _cx: &Context) -> Reply {
        Reply::say("확인했습니다.")
    }
}

/// UI로 돌려주는 전체 응답
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub understanding: Understanding,
    /// 확신도가 낮아서 unknown으로 바꿨는지
    pub fallback: bool,
    pub reply: Reply,
}

/// 두뇌 + 스킬 묶음. 앱은 이것만 쓰면 됨.
pub struct Assistant {
    brain: Box<dyn Brain>,
    talker: Option<Arc<dyn Talker>>,
    skills: Vec<Box<dyn Skill>>,
    route: HashMap<String, usize>,
    pub config: Arc<RwLock<Config>>,
    pub data_dir: PathBuf,
    events: EventSink,
}

impl Assistant {
    pub fn new(
        brain: Box<dyn Brain>,
        talker: Option<Arc<dyn Talker>>,
        data_dir: PathBuf,
        events: EventSink,
    ) -> Self {
        let config = Arc::new(RwLock::new(Config::load_or_create(&data_dir)));
        let mut a = Self {
            brain,
            talker,
            skills: vec![],
            route: HashMap::new(),
            config,
            data_dir,
            events,
        };
        for s in skills::builtin() {
            a.register(s);
        }
        a
    }

    pub fn register(&mut self, skill: Box<dyn Skill>) {
        let idx = self.skills.len();
        for i in skill.intents() {
            self.route.insert(i.to_string(), idx);
        }
        self.skills.push(skill);
    }

    fn ctx(&self, text: &str) -> Context {
        Context {
            text: text.to_string(),
            config: self.config.clone(),
            data_dir: self.data_dir.clone(),
            events: self.events.clone(),
            talker: self.talker.clone(),
        }
    }

    pub fn handle(&self, text: &str) -> Response {
        let u = self.brain.understand(text);
        let threshold = self.config.read().unwrap().threshold;
        let fallback = u.confidence < threshold;
        let intent = if fallback {
            "unknown"
        } else {
            u.intent.as_str()
        };
        let cx = self.ctx(text);
        let reply = match self.route.get(intent) {
            Some(&i) => self.skills[i].run(intent, &u, &cx),
            None => Reply::error(format!("'{intent}' 명령을 처리할 스킬이 아직 없습니다.")),
        };
        Response {
            understanding: u,
            fallback,
            reply,
        }
    }

    pub fn confirm(&self, intent: &str) -> Reply {
        match self.route.get(intent) {
            Some(&i) => self.skills[i].confirmed(intent, &self.ctx("")),
            None => Reply::error("확인할 내용이 없습니다."),
        }
    }

    pub fn brain_name(&self) -> String {
        self.brain.name()
    }

    pub fn talker_name(&self) -> Option<String> {
        self.talker.as_ref().map(|t| t.name())
    }

    /// 두뇌는 아는데 스킬이 없는 의도 (개발할 때 확인용)
    pub fn missing_skills(&self) -> Vec<String> {
        self.brain
            .intents()
            .iter()
            .filter(|i| !self.route.contains_key(*i))
            .cloned()
            .collect()
    }

    pub fn capabilities(&self) -> Vec<&'static str> {
        self.skills
            .iter()
            .map(|s| s.describe())
            .filter(|d| !d.is_empty())
            .collect()
    }
}

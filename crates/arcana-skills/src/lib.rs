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
pub mod knowledge;
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
    /// 스스로 공부해서 새 문서를 배움 (학습 탭에 표시)
    Learned { title: String, count: usize },
    /// 학습 탭에 상태 한 줄
    StudyLog { text: String },
    /// Arcana가 사용자에게 질문 (학습 탭)
    Question { text: String },
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
    /// 지식 저장소 (공책)
    pub knowledge: Option<Arc<knowledge::Knowledge>>,
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
    /// 대화 모델이 만든 대답인지 (UI가 👍/👎 버튼을 붙임)
    pub from_chat: bool,
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
    pub fn chat(t: impl Into<String>) -> Self {
        Self {
            text: t.into(),
            from_chat: true,
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

/// 학습 탭 응답
#[derive(Debug, Clone, Serialize)]
pub struct StudyResult {
    pub lines: Vec<String>,
    pub running: bool,
    pub question: Option<String>,
    pub stats: knowledge::Stats,
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
    pub knowledge: Arc<knowledge::Knowledge>,
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
        let knowledge = knowledge::Knowledge::open(data_dir.join("knowledge"));
        let mut a = Self {
            brain,
            talker,
            knowledge,
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
            knowledge: Some(self.knowledge.clone()),
        }
    }

    pub fn handle(&self, text: &str) -> Response {
        let u = self.brain.understand(text);
        // 1) 사용자가 직접 가르쳐 준 말이면 그 대답
        if let Some(r) = self.knowledge.taught_reply(text) {
            let u = Understanding {
                intent: "taught".into(),
                confidence: 1.0,
                slots: Default::default(),
            };
            return Response {
                understanding: u,
                fallback: false,
                reply: Reply::chat(r),
            };
        }
        let threshold = self.config.read().unwrap().threshold;
        let fallback = u.confidence < threshold;
        // 2) 확신이 낮으면(잘 못 알아들음) 학습 탭에서 어떻게 대답할지 물어보기
        if fallback && text.trim().chars().count() >= 2 {
            let q = knowledge::Question::Reply {
                phrase: text.trim().to_string(),
            };
            if self.knowledge.ask(q.clone()) {
                self.emit_question();
            }
        }
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

    /// 학습 탭 입력 처리: /Start, /Stop, /Status, /Skip, /Teach, /Help 또는 질문에 대한 대답
    pub fn study(&self, input: &str) -> StudyResult {
        let input = input.trim();
        let k = &self.knowledge;
        let mut lines = Vec::new();
        let set_running = |on: bool| {
            let mut c = self.config.write().unwrap();
            c.self_study = on;
            c.save(&self.data_dir);
        };
        if let Some(cmd) = input.strip_prefix('/') {
            let (name, rest) = cmd.split_once(' ').unwrap_or((cmd, ""));
            match name.to_lowercase().as_str() {
                "start" | "시작" => {
                    set_running(true);
                    lines.push("학습을 시작합니다. 앱이 켜져 있는 동안 위키백과를 조금씩 읽으며 공부하겠습니다.".into());
                    lines.push("모르는 게 생기면 여기서 여쭤보겠습니다.".into());
                }
                "stop" | "중지" | "멈춰" => {
                    set_running(false);
                    lines.push(
                        "학습을 멈췄습니다. 다시 시작하시려면 /Start 를 입력해 주십시오.".into(),
                    );
                }
                "status" | "상태" => {
                    let s = k.stats();
                    lines.push(format!(
                        "{} · 알고 있는 문서 {}개 · 읽을 예정 {}개 · 배운 대답 {}개 · 남은 질문 {}개 · 피드백 👍{} 👎{}",
                        if self.config.read().unwrap().self_study { "학습 중" } else { "멈춤" },
                        s.count, s.queued, s.taught, s.questions, s.likes, s.dislikes
                    ));
                    if !s.recent.is_empty() {
                        lines.push(format!("최근에 배운 것: {}", s.recent.join(", ")));
                    }
                }
                "skip" | "넘기기" => {
                    if k.current_question().is_some() {
                        k.pop_question();
                        lines.push("이 질문은 넘기겠습니다.".into());
                    } else {
                        lines.push("넘길 질문이 없습니다.".into());
                    }
                }
                "teach" | "가르치기" => match rest.split_once('=') {
                    Some((a, b)) if !a.trim().is_empty() && !b.trim().is_empty() => {
                        k.teach(a, b);
                        lines.push(format!(
                            "알겠습니다. '{}'라고 하시면 '{}'라고 대답하겠습니다.",
                            a.trim(),
                            b.trim()
                        ));
                    }
                    _ => lines.push(
                        "형식: /Teach 말 = 대답   (예: /Teach 배고파 = 뭐 드시고 싶으십니까?)"
                            .into(),
                    ),
                },
                _ => {
                    lines.push("사용할 수 있는 명령입니다.".into());
                    lines.push("/Start  스스로 학습 시작".into());
                    lines.push("/Stop   학습 멈추기".into());
                    lines.push("/Status 지금까지 배운 것".into());
                    lines.push("/Skip   지금 질문 넘기기".into());
                    lines.push("/Teach 말 = 대답   대답 직접 가르치기".into());
                    lines
                        .push("그 외에 입력하시는 말은 제 질문에 대한 대답으로 받겠습니다.".into());
                    lines.push("대화 탭의 👍/👎 피드백은 training\\피드백_학습하기.bat 으로 모델에 학습시킬 수 있습니다.".into());
                }
            }
        } else if !input.is_empty() {
            match k.answer(input) {
                Ok(m) => lines.push(m),
                Err(e) => lines.push(e),
            }
        }
        self.study_state(lines)
    }

    /// 대답 평가 (👍 = 1, 👎 = -1, better = 👎일 때 더 좋은 대답)
    pub fn feedback(&self, user: &str, reply: &str, rating: i8, better: Option<&str>) -> String {
        let msg = self.knowledge.add_feedback(user, reply, rating, better);
        if rating < 0 && better.map_or(true, |b| b.trim().is_empty()) {
            self.emit_question();
        }
        msg
    }

    fn emit_question(&self) {
        if let Some(q) = self.knowledge.current_question() {
            (self.events)(Event::Question { text: q.text() });
        }
    }

    pub fn study_state(&self, lines: Vec<String>) -> StudyResult {
        StudyResult {
            lines,
            running: self.config.read().unwrap().self_study,
            question: self.knowledge.current_question().map(|q| q.text()),
            stats: self.knowledge.stats(),
        }
    }

    /// 백그라운드 학습 스레드 시작 (실제로 공부할지는 /Start, /Stop 으로 결정)
    pub fn start_self_study(&self) {
        self.knowledge
            .start_explorer(self.config.clone(), self.events.clone());
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

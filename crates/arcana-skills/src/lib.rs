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
pub mod crawler;
pub mod knowledge;
pub mod net;
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
    Learned {
        title: String,
        site: String,
        count: usize,
    },
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

/// 의도별로 문장에 있어야 하는 말 (띄어쓰기 없이 비교). 목록에 없는 의도는 검사 안 함
fn required_words(intent: &str) -> &'static [&'static [&'static str]] {
    match intent {
        "open_app" => &[&[
            "켜",
            "열어",
            "열기",
            "실행",
            "띄워",
            "키자",
            "들어가",
            "시작",
            "켤",
        ]],
        "close_app" => &[&["꺼", "끄", "닫", "종료", "끝내", "끌"]],
        "lock" => &[&["잠가", "잠금", "잠궈", "잠그", "락"]],
        "shutdown" => &[
            &["꺼", "끄", "종료", "끌"],
            &["컴퓨터", "컴", "pc", "전원", "시스템"],
        ],
        "volume_up" | "volume_down" => &[&["소리", "볼륨"]],
        "mute" => &[&["음소거", "뮤트", "소리꺼", "소리끄", "소리없", "조용히"]],
        "screenshot" => &[&["스크린샷", "캡처", "캡쳐", "스샷", "화면찍", "화면저장"]],
        "set_timer" => &[
            &[
                "타이머",
                "알람",
                "알려",
                "깨워",
                "불러",
                "재줘",
                "카운트",
                "말해",
                "지나면",
            ],
            &["초", "분", "시간"],
        ],
        "memo_add" => &[&["메모", "적어", "기억해", "할일", "추가"]],
        "memo_read" => &[&["메모", "할일", "적어둔"]],
        "memo_clear" => &[&["메모", "할일"], &["지워", "삭제", "초기화", "비워"]],
        "youtube" => &[&["유튜브", "틀어", "재생", "보여줘", "듣고싶", "보고싶"]],
        "web_search" => &[&["검색", "찾아", "구글", "알아봐", "네이버", "구글링"]],
        "ask_time" => &[&["시", "시간"]],
        "ask_date" => &[&["며칠", "몇일", "날짜", "요일", "몇월"]],
        _ => &[],
    }
}

pub fn intent_has_action_words(intent: &str, text: &str) -> bool {
    let t: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    required_words(intent)
        .iter()
        .all(|group| group.iter().any(|w| t.contains(w)))
}

/// 사용자가 학습한 대화 모델 (%APPDATA%\Arcana\models\arcana-chat.*). 없으면 Ok(None)
pub fn load_talker(data_dir: &std::path::Path) -> Result<Option<arcana_core::ArcanaChat>, String> {
    let m = data_dir.join("models");
    let (w, j) = (
        m.join("arcana-chat.safetensors"),
        m.join("arcana-chat.json"),
    );
    if !w.exists() || !j.exists() {
        return Ok(None);
    }
    let weights = std::fs::read(&w).map_err(|e| e.to_string())?;
    let meta = std::fs::read_to_string(&j).map_err(|e| e.to_string())?;
    arcana_core::ArcanaChat::from_bytes(&weights, &meta)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// 두뇌 + 스킬 묶음. 앱은 이것만 쓰면 됨.
pub struct Assistant {
    brain: Box<dyn Brain>,
    /// 대화 모델 (/Reload 로 바꿔 끼울 수 있게 RwLock)
    talker: RwLock<Option<Arc<dyn Talker>>>,
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
            talker: RwLock::new(talker),
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
            talker: self.talker.read().unwrap().clone(),
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
        // 2) 웹 주소가 들어 있으면 그 페이지 읽기
        if let Some(url) = skills::web::find_url(text) {
            let mut slots = std::collections::BTreeMap::new();
            slots.insert("URL".to_string(), url);
            let u = Understanding {
                intent: "read_page".into(),
                confidence: 1.0,
                slots,
            };
            let reply = match self.route.get("read_page") {
                Some(&i) => self.skills[i].run("read_page", &u, &self.ctx(text)),
                None => Reply::error("페이지 읽기 스킬이 없습니다."),
            };
            return Response {
                understanding: u,
                fallback: false,
                reply,
            };
        }
        let threshold = self.config.read().unwrap().threshold;
        // 안전장치: 실제로 무언가를 실행하는 명령은 문장에 그 동작 말이 있을 때만
        //   ("어제 잠 못 잤어?" → 화면 잠금 X, "디코 들어와" → 디스코드 끄기 X)
        let blocked = !intent_has_action_words(&u.intent, text);
        let fallback = u.confidence < threshold || blocked;
        // 3) 확신이 낮으면(잘 못 알아들음) 학습 탭에서 어떻게 대답할지 물어보기
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
                    lines.push("학습을 시작합니다. 앱이 켜져 있는 동안 위키백과(한국어·영어·일본어)와 여러 웹사이트를 조금씩 읽으며 공부하겠습니다.".into());
                    lines.push("쉬지 않고 계속 읽습니다. 사이트 규칙(robots.txt)은 지키고, 위험한 사이트는 건너뜁니다.".into());
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
                "reload" | "새로고침" => lines.push(self.reload_talker()),
                "train" | "학습" => {
                    lines.extend(self.start_training(rest.trim().trim_matches('"')))
                }
                "diag" | "진단" => {
                    lines.push("인터넷 연결을 확인하는 중입니다…".into());
                    lines.extend(net::diagnose());
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
                    lines.push("/Diag   인터넷 연결 진단".into());
                    lines.push(
                        "/Train  공부한 문서로 대화 모델 학습 (처음 한 번은 /Train 학습폴더경로)"
                            .into(),
                    );
                    lines.push("/Reload 새로 학습한 대화 모델 바로 적용".into());
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

    /// %APPDATA%\Arcana\models 의 대화 모델 다시 불러오기
    pub fn reload_talker(&self) -> String {
        match load_talker(&self.data_dir) {
            Ok(Some(t)) => {
                let name = t.name();
                *self.talker.write().unwrap() = Some(Arc::new(t));
                format!("새 대화 모델을 적용했습니다. ({name})")
            }
            Ok(None) => "아직 학습된 대화 모델이 없습니다. /Train 으로 학습해 주십시오.".into(),
            Err(e) => format!("대화 모델을 불러오지 못했습니다. {e}"),
        }
    }

    /// 학습 창 열기: training 폴더의 공책으로_학습하기.bat 실행
    fn start_training(&self, path: &str) -> Vec<String> {
        if !path.is_empty() {
            let mut c = self.config.write().unwrap();
            c.training_dir = path.to_string();
            c.save(&self.data_dir);
        }
        let dir = std::path::PathBuf::from(self.config.read().unwrap().training_dir.clone());
        if dir.as_os_str().is_empty() || !dir.join("pretrain_notes.py").exists() {
            return vec![
                "학습 도구가 있는 폴더를 알려 주십시오. (Arcana 레포의 training 폴더)".into(),
                "예: /Train C:\\Users\\이름\\Arcana\\training".into(),
                "레포가 없으시면: git clone https://github.com/XxErone2xX/Arcana".into(),
            ];
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let r = std::process::Command::new("cmd")
                .raw_arg(format!(
                    "/C start \"Arcana 학습\" /D \"{}\" \"공책으로_학습하기.bat\"",
                    dir.display()
                ))
                .spawn();
            return match r {
                Ok(_) => vec![
                    "학습 창을 열었습니다. 공부한 문서로 한국어를 배운 뒤 대화 데이터로 말하는 법을 배웁니다.".into(),
                    "끝나면 /Reload 를 입력해 주십시오.".into(),
                ],
                Err(e) => vec![format!("학습 창을 열지 못했습니다. ({e})")],
            };
        }
        #[cfg(not(windows))]
        vec![format!(
            "이 컴퓨터에서는 직접 실행해 주십시오: cd \"{}\" && python pretrain_notes.py",
            dir.display()
        )]
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
        self.talker.read().unwrap().as_ref().map(|t| t.name())
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

#[cfg(test)]
mod safety_tests {
    use super::intent_has_action_words as ok;
    #[test]
    fn action_words() {
        assert!(!ok("lock", "어제 잠 못 잤어?"));
        assert!(ok("lock", "컴퓨터 잠가줘"));
        assert!(!ok("close_app", "그럼 끝나면 디코 들어와"));
        assert!(ok("close_app", "디코 꺼줘"));
        assert!(!ok("open_app", "너 고양이 키운다며?"));
        assert!(ok("open_app", "크롬 켜줘"));
        assert!(!ok("shutdown", "불 꺼줘"));
        assert!(ok("shutdown", "컴퓨터 꺼줘"));
        assert!(!ok("set_timer", "너 그거 들었어? 다은이 전학 간대."));
        assert!(ok("set_timer", "라면 3분 타이머"));
        assert!(!ok("screenshot", "태현이 찍어줬어."));
        assert!(ok("greet", "아무 말"));
    }
}

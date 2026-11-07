//! 사용자 설정 (%APPDATA%\Arcana\config.json)
//! 처음 실행하면 기본값으로 만들어짐. 메모장으로 고쳐도 되고, 앱 설정 화면에서 열 수도 있음.
//! 새 항목을 추가해도 예전 설정 파일이 안 깨지게 전부 #[serde(default)] 처리.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 설정 파일 형식 버전 (나중에 구조 바뀌면 자동 변환용)
    pub version: u32,
    pub name: String,
    /// 이 확신도보다 낮으면 "모르겠다"고 대답
    pub threshold: f32,
    pub auto_update: bool,
    /// AI가 어떻게 판단했는지 대화창에 표시
    pub show_debug: bool,
    /// 스스로 공부 중인지 (학습 탭의 /Start, /Stop). 켜두면 앱 다시 켜도 이어서 공부
    pub self_study: bool,
    /// 자율 탐색 간격 (초). 0 = 쉬지 않고 계속
    pub study_interval_secs: u64,
    /// 공부할 위키백과 언어들 (ko=한국어, en=영어, ja=일본어 ...)
    pub wiki_langs: Vec<String>,
    /// 위키백과 말고 일반 웹사이트도 돌아다닐지
    pub explore_web: bool,
    /// 절대 안 갈 사이트 (예: "example.com" → 하위 주소 전부 차단)
    pub blocked_domains: Vec<String>,
    /// 같은 사이트 최소 간격 (초). 0 = 없음. 사이트가 robots.txt 로 Crawl-delay 를 정해 두면 그건 따름
    pub site_delay_secs: u64,
    /// 하루에 한 사이트에서 읽을 최대 페이지 수
    pub site_daily_limit: usize,
    /// 생각 깊이: 대답 후보를 몇 개 만들어서 고를지 (클수록 좋은 대답, 대신 느려짐. 1~32)
    pub think_depth: usize,
    /// 대답 다양성 (0.5 = 늘 비슷, 1.2 = 아주 다양하지만 가끔 이상함)
    pub chat_temperature: f32,
    /// 한 글자를 고를 때 후보 글자 수 (클수록 다양)
    pub chat_top_k: usize,
    /// 비슷하면 긴 대답을 우대
    pub prefer_long_replies: bool,
    /// 생각 수치 1~5 (모델 탭 슬라이더). 5 = PrimeThink: 스스로 완벽하다고 할 때까지 생각
    pub think_level: u8,
    /// 메모리 켜기 (이름·좋아하는 것 등 기억)
    pub memory_enabled: bool,
    /// 컨테이너 메모리 켜기 (대화 기록 저장 → 앱을 다시 켜도 맥락 유지)
    pub container_enabled: bool,
    /// 컨테이너 메모리 최대 크기 (MB, 10 ~ 10240)
    pub container_mb: u64,
    /// 빠른 호출 단축키 (어느 화면에서든 Arcana 부르기). 예: "Alt+Shift+A", "Ctrl+Space", "Alt+Q"
    pub hotkey: String,
    pub hotkey_enabled: bool,
    /// 학습 도구 폴더 (Arcana 레포의 training 폴더). /Train 이 여기 있는 bat 을 실행
    pub training_dir: String,
    /// (선택) Brave Search API 키 — 넣으면 채팅 검색이 일반 웹 전체에서 됨. 없으면 위키백과 + DuckDuckGo 요약
    pub brave_api_key: String,
    pub apps: BTreeMap<String, AppEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppEntry {
    /// 실행할 것: exe 이름, 전체 경로, URL, 프로토콜(discord:// 등)
    pub open: String,
    /// 끌 때 종료할 프로세스 이름들 (비워두면 끄기 금지)
    pub close: Vec<String>,
    /// 다르게 부르는 이름들
    pub aliases: Vec<String>,
}

fn app(open: &str, close: &[&str], aliases: &[&str]) -> AppEntry {
    AppEntry {
        open: open.into(),
        close: close.iter().map(|s| s.to_string()).collect(),
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut apps = BTreeMap::new();
        let mut add = |k: &str, e| {
            apps.insert(k.to_string(), e);
        };
        add(
            "크롬",
            app(
                "chrome",
                &["chrome.exe"],
                &["chrome", "인터넷", "구글", "크롬창"],
            ),
        );
        add("엣지", app("msedge", &["msedge.exe"], &["edge"]));
        add("메모장", app("notepad", &["notepad.exe"], &["notepad"]));
        add(
            "계산기",
            app("calc", &["CalculatorApp.exe", "Calculator.exe"], &["calc"]),
        );
        add(
            "디스코드",
            app("discord://", &["Discord.exe"], &["디코", "discord"]),
        );
        add(
            "로블록스",
            app(
                "https://www.roblox.com/home",
                &["RobloxPlayerBeta.exe"],
                &["roblox", "롤블록스"],
            ),
        );
        add(
            "로블록스 스튜디오",
            app(
                "roblox-studio:",
                &["RobloxStudioBeta.exe"],
                &["스튜디오", "roblox studio"],
            ),
        );
        add(
            "탐색기",
            app("explorer", &[], &["파일 탐색기", "파일", "explorer"]),
        );
        add(
            "스포티파이",
            app("spotify:", &["Spotify.exe"], &["spotify"]),
        );
        add("그림판", app("mspaint", &["mspaint.exe"], &["paint"]));
        add(
            "작업 관리자",
            app("taskmgr", &["Taskmgr.exe"], &["작업관리자"]),
        );
        add(
            "vscode",
            app(
                "code",
                &["Code.exe"],
                &["비주얼 스튜디오 코드", "vs코드", "코드"],
            ),
        );
        add(
            "카카오톡",
            app(
                r"C:\Program Files (x86)\Kakao\KakaoTalk\KakaoTalk.exe",
                &["KakaoTalk.exe"],
                &["카톡"],
            ),
        );
        add("스팀", app("steam://open/main", &["steam.exe"], &["steam"]));
        add(
            "설정",
            app("ms-settings:", &["SystemSettings.exe"], &["윈도우 설정"]),
        );
        add("명령 프롬프트", app("cmd", &[], &["cmd", "명령프롬프트"]));
        add("파워쉘", app("powershell", &[], &["powershell"]));
        add("블렌더", app("blender", &["blender.exe"], &["blender"]));
        add(
            "오토핫키",
            app(
                "autohotkey",
                &["AutoHotkey.exe", "AutoHotkeyU64.exe", "AutoHotkeyU32.exe"],
                &["ahk"],
            ),
        );
        Self {
            version: 4,
            name: "Arcana".into(),
            threshold: 0.55,
            auto_update: true,
            show_debug: false,
            self_study: false,
            study_interval_secs: 0,
            wiki_langs: vec!["ko".into(), "en".into(), "ja".into()],
            explore_web: true,
            blocked_domains: vec![],
            site_delay_secs: 0,
            site_daily_limit: 100,
            brave_api_key: String::new(),
            training_dir: String::new(),
            think_depth: 8,
            chat_temperature: 0.9,
            chat_top_k: 16,
            prefer_long_replies: true,
            think_level: 3,
            memory_enabled: true,
            container_enabled: true,
            container_mb: 100,
            hotkey: "Alt+Shift+A".into(),
            hotkey_enabled: true,
            apps,
        }
    }
}

/// 생각 수치 → (후보 수, 다듬기 횟수, PrimeThink)
pub struct Think {
    pub candidates: usize,
    pub rounds: usize,
    pub prime: bool,
}

impl Config {
    pub fn think(&self) -> Think {
        match self.think_level.clamp(1, 5) {
            1 => Think { candidates: 2, rounds: 1, prime: false },
            2 => Think { candidates: 4, rounds: 1, prime: false },
            3 => Think { candidates: 8, rounds: 1, prime: false },
            4 => Think { candidates: 16, rounds: 2, prime: false },
            _ => Think { candidates: 16, rounds: 12, prime: true },
        }
    }

    pub fn path(dir: &Path) -> std::path::PathBuf {
        dir.join("config.json")
    }

    pub fn load_or_create(dir: &Path) -> Self {
        let p = Self::path(dir);
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str::<Self>(&s)
                .map(|mut c| {
                    // v1 → v2: 탐색 쿨타임 없앰 (예전 기본값 20초/10초 → 0초)
                    if c.version < 2 {
                        if c.study_interval_secs == 20 {
                            c.study_interval_secs = 0;
                        }
                        if c.site_delay_secs == 10 {
                            c.site_delay_secs = 0;
                        }
                        c.version = 2;
                        c.save(dir);
                    }
                    // v2 → v3: 메모리·생각 수치 추가 (serde default 가 false/0 을 넣으므로 켜 줌)
                    if c.version < 3 {
                        c.think_level = 3;
                        c.memory_enabled = true;
                        c.container_enabled = true;
                        c.container_mb = 100;
                        c.hotkey = "Alt+Shift+A".into();
                        c.hotkey_enabled = true;
                        c.version = 3;
                        c.save(dir);
                    }
                    // v3 → v4: 빠른 호출 기본 단축키 Shift+A → Alt+Shift+A (대문자 A 입력과 안 겹치게)
                    if c.version < 4 {
                        if c.hotkey == "Shift+A" {
                            c.hotkey = "Alt+Shift+A".into();
                        }
                        c.version = 4;
                        c.save(dir);
                    }
                    c
                })
                .unwrap_or_else(|e| {
                    // 설정 파일이 깨졌으면 백업해두고 기본값으로
                    let _ = std::fs::rename(&p, dir.join("config.broken.json"));
                    eprintln!("config.json 읽기 실패 ({e}), 기본값 사용");
                    let c = Self::default();
                    c.save(dir);
                    c
                }),
            Err(_) => {
                let c = Self::default();
                c.save(dir);
                c
            }
        }
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(Self::path(dir), serde_json::to_string_pretty(self).unwrap());
    }
}

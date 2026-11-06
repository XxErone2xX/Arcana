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
    /// 자율 탐색 간격 (초). 너무 짧으면 위키백과 서버에 부담
    pub study_interval_secs: u64,
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
            version: 1,
            name: "Arcana".into(),
            threshold: 0.55,
            auto_update: true,
            show_debug: false,
            self_study: false,
            study_interval_secs: 20,
            apps,
        }
    }
}

impl Config {
    pub fn path(dir: &Path) -> std::path::PathBuf {
        dir.join("config.json")
    }

    pub fn load_or_create(dir: &Path) -> Self {
        let p = Self::path(dir);
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
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

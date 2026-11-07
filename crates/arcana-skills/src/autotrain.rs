//! 자동 학습: /Start 로 켜 둔 동안, 새로 모인 재료(문서·대화)가 충분하고
//! PC를 한동안 안 쓰고 있으면 알아서 대화 모델을 학습하고 바로 적용(/Reload)함.
//!
//! - 학습은 보이지 않는 창에서, 낮은 우선순위로 돌아감 (기록: %APPDATA%\Arcana\training\auto.log)
//! - 끝나면 새 대화 모델을 바로 적용
//! - 재료가 부족하면 (대화 데이터 없음 등) 새 재료가 생길 때까지 다시 시도하지 않음

use crate::{Assistant, Event};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 학습 재료 개수
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Material {
    pub docs: usize,
    /// training/data/*.jsonl 대화 쌍
    pub pairs: usize,
    /// 직접 가르친 대답 + 👎 로 알려 준 더 좋은 대답
    pub mine: usize,
}

impl Material {
    /// pretrain_notes.py 가 학습을 시작할 수 있는 최소 조건과 같음
    pub fn trainable(&self) -> bool {
        self.docs >= 50 && (self.pairs > 0 || self.mine >= 30)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AutoState {
    /// 마지막으로 학습에 성공했을 때의 재료
    trained: Material,
    /// 마지막으로 시도했지만 재료 부족·실패였을 때의 재료 (같으면 다시 시도 안 함)
    failed: Option<Material>,
    last_time: String,
}

pub static RUNNING: AtomicBool = AtomicBool::new(false);

fn state_path(dir: &Path) -> PathBuf {
    dir.join("training").join("auto_state.json")
}

fn load(dir: &Path) -> AutoState {
    std::fs::read_to_string(state_path(dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(dir: &Path, s: &AutoState) {
    let _ = std::fs::create_dir_all(dir.join("training"));
    let _ = std::fs::write(state_path(dir), serde_json::to_string_pretty(s).unwrap());
}

fn count_lines(p: &Path, needle: Option<&str>) -> usize {
    std::fs::read_to_string(p)
        .map(|s| s.lines().filter(|l| !l.trim().is_empty() && needle.map_or(true, |n| l.contains(n))).count())
        .unwrap_or(0)
}

pub fn material(a: &Assistant) -> Material {
    let ddir = a.data_dir.join("training").join("data");
    let pairs = std::fs::read_dir(&ddir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().extension().map_or(false, |x| x == "jsonl"))
                .map(|e| count_lines(&e.path(), Some("\"bot\"")))
                .sum()
        })
        .unwrap_or(0);
    let kdir = a.data_dir.join("knowledge");
    let mine = count_lines(&kdir.join("taught.jsonl"), None) + count_lines(&kdir.join("feedback.jsonl"), Some("\"better\":\""));
    Material { docs: a.knowledge.stats().count, pairs, mine }
}

/// PC를 안 쓴 시간 (초). 윈도우가 아니면 항상 "안 쓰는 중"으로 봄
pub fn idle_secs() -> u64 {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::SystemInformation::GetTickCount;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if GetLastInputInfo(&mut li) != 0 {
            return (GetTickCount().wrapping_sub(li.dwTime) / 1000) as u64;
        }
        0
    }
    #[cfg(not(windows))]
    {
        u64::MAX
    }
}

/// 학습할 때가 됐는지. 이유 문장을 돌려줌
pub fn due(a: &Assistant) -> Option<String> {
    let (on, study, docs_n, pairs_n, idle_min) = {
        let c = a.config.read().unwrap();
        (c.auto_train, c.self_study, c.auto_train_docs, c.auto_train_pairs, c.auto_train_idle_min)
    };
    if !on || !study || RUNNING.load(Ordering::SeqCst) {
        return None;
    }
    let m = material(a);
    if !m.trainable() {
        return None;
    }
    let st = load(&a.data_dir);
    if st.failed == Some(m) {
        return None; // 지난번과 재료가 같으면 다시 해도 똑같이 실패
    }
    let new_docs = m.docs.saturating_sub(st.trained.docs);
    let new_pairs = (m.pairs + m.mine).saturating_sub(st.trained.pairs + st.trained.mine);
    let first = st.trained == Material::default();
    if !(first || new_docs >= docs_n.max(1) || new_pairs >= pairs_n.max(1)) {
        return None;
    }
    if idle_secs() < idle_min * 60 {
        return None; // PC를 쓰는 중이면 기다림
    }
    Some(format!("새 문서 {new_docs}개 · 새 대화 {new_pairs}쌍"))
}

/// 학습 실행 (끝날 때까지 기다림). 성공하면 true
fn run(a: &Arc<Assistant>, why: &str) -> bool {
    let emit = |t: String| (a.events)(Event::StudyLog { text: t });
    let m = material(a);
    let dir = match crate::install_training_tools(&a.data_dir) {
        Ok(d) => d,
        Err(e) => {
            emit(format!("자동 학습 준비에 실패했습니다. ({e})"));
            return false;
        }
    };
    emit(format!("PC를 쓰지 않는 동안 대화 모델을 학습합니다. ({why}) 끝나면 바로 적용하겠습니다."));
    let log = std::fs::File::create(dir.join("auto.log"));
    let status = {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
            let mut cmd = std::process::Command::new("cmd");
            cmd.raw_arg("/C \"공책으로_학습하기.bat\"")
                .current_dir(&dir)
                .env("ARCANA_AUTO", "1")
                .env("PYTHONIOENCODING", "utf-8")
                .creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
            if let Ok(f) = log {
                if let Ok(f2) = f.try_clone() {
                    cmd.stdout(f).stderr(f2);
                }
            }
            cmd.status()
        }
        #[cfg(not(windows))]
        {
            let mut cmd = std::process::Command::new("python3");
            cmd.arg("pretrain_notes.py").current_dir(&dir);
            if let Ok(f) = log {
                if let Ok(f2) = f.try_clone() {
                    cmd.stdout(f).stderr(f2);
                }
            }
            cmd.status()
        }
    };
    let mut st = load(&a.data_dir);
    st.last_time = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let code = status.as_ref().ok().and_then(|s| s.code());
    let ok = code == Some(0);
    if ok {
        st.trained = m;
        st.failed = None;
        save(&a.data_dir, &st);
        emit(format!("자동 학습을 마쳤습니다. {}", a.reload_talker()));
    } else {
        st.failed = Some(m);
        save(&a.data_dir, &st);
        emit(match code {
            Some(2) => "자동 학습: 재료가 부족해서 학습하지 않았습니다. 대화 데이터나 가르친 대답이 더 모이면 다시 시도하겠습니다.".into(),
            Some(9) => "자동 학습: 이 PC에 Python이 없습니다. python.org 에서 설치해 주십시오 (Add to PATH 체크).".into(),
            _ => format!("자동 학습이 실패했습니다. 자세한 내용: {}", dir.join("auto.log").display()),
        });
    }
    ok
}

/// 1분마다 확인하는 감시 스레드 시작
pub fn start(a: Arc<Assistant>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        if let Some(why) = due(&a) {
            RUNNING.store(true, Ordering::SeqCst);
            run(&a, &why);
            RUNNING.store(false, Ordering::SeqCst);
        }
    });
}

/// /Status 에 보여줄 한 줄
pub fn status_line(a: &Assistant) -> String {
    let c = a.config.read().unwrap().clone();
    if !c.auto_train {
        return "자동 학습 꺼짐".into();
    }
    if RUNNING.load(Ordering::SeqCst) {
        return "자동 학습 중 (대화 모델 학습 중)".into();
    }
    let m = material(a);
    let st = load(&a.data_dir);
    let mut s = format!(
        "자동 학습 켜짐 · 대화 재료 {}쌍 · PC를 {}분 안 쓰면 학습",
        m.pairs + m.mine,
        c.auto_train_idle_min
    );
    if !m.trainable() {
        s += " · 아직 재료 부족 (대화 데이터 또는 가르친 대답 30개 필요)";
    }
    if !st.last_time.is_empty() {
        s += &format!(" · 마지막 학습 {}", st.last_time);
    }
    s
}

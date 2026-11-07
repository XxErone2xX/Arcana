// 배포 버전에서는 검은 콘솔 창 안 뜨게
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod updater;

use arcana_skills::{Assistant, Event, Reply, Response, StudyResult};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

struct AppState {
    assistant: Arc<Assistant>,
    update: Mutex<Option<updater::UpdateInfo>>,
    hotkey_error: Mutex<Option<String>>,
}

#[derive(Serialize)]
struct Info {
    name: String,
    app_name: String,
    version: String,
    brain: String,
    talker: Option<String>,
    capabilities: Vec<&'static str>,
    show_debug: bool,
    auto_update: bool,
    data_dir: String,
    missing_skills: Vec<String>,
    think_level: u8,
    memory_enabled: bool,
    container_enabled: bool,
    container_mb: u64,
    hotkey: String,
    hotkey_enabled: bool,
    /// 단축키 등록 실패 이유 (다른 프로그램이 쓰는 중 등)
    hotkey_error: Option<String>,
}

#[derive(Serialize)]
struct ContainerInfo {
    used_bytes: u64,
    max_mb: u64,
    enabled: bool,
    turns: usize,
}

// ───────────────────── UI에서 부르는 명령들 (ui/js/api.js) ─────────────────────

/// 대화 모델이 커지면 대답 만드는 데 시간이 걸려서, 창이 안 멈추게 별도 스레드에서 실행
#[tauri::command]
async fn send(state: State<'_, AppState>, text: String) -> Result<Response, String> {
    let a = state.assistant.clone();
    tauri::async_runtime::spawn_blocking(move || a.handle(&text))
        .await
        .map_err(|e| e.to_string())
}

/// 학습 탭 입력 (/Start, /Stop, 질문에 대한 대답 등). 위키백과 접속이 있을 수 있어서 별도 스레드
#[tauri::command]
async fn study(state: State<'_, AppState>, text: String) -> Result<StudyResult, String> {
    let a = state.assistant.clone();
    tauri::async_runtime::spawn_blocking(move || a.study(&text))
        .await
        .map_err(|e| e.to_string())
}

/// 대답 평가 (👍/👎)
#[tauri::command]
fn feedback(
    state: State<AppState>,
    user: String,
    reply: String,
    rating: i8,
    better: Option<String>,
) -> String {
    state
        .assistant
        .feedback(&user, &reply, rating, better.as_deref())
}

#[tauri::command]
fn study_status(state: State<AppState>) -> StudyResult {
    state.assistant.study_state(vec![])
}

#[tauri::command]
fn confirm(state: State<AppState>, intent: String) -> Reply {
    state.assistant.confirm(&intent)
}

#[tauri::command]
fn info(app: AppHandle, state: State<AppState>) -> Info {
    let a = &state.assistant;
    let c = a.config.read().unwrap();
    Info {
        name: c.name.clone(),
        app_name: app.package_info().name.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        brain: a.brain_name(),
        talker: a.talker_name(),
        capabilities: a.capabilities(),
        show_debug: c.show_debug,
        auto_update: c.auto_update,
        data_dir: a.data_dir.display().to_string(),
        missing_skills: a.missing_skills(),
        think_level: c.think_level.clamp(1, 5),
        memory_enabled: c.memory_enabled,
        container_enabled: c.container_enabled,
        container_mb: c.container_mb,
        hotkey: c.hotkey.clone(),
        hotkey_enabled: c.hotkey_enabled,
        hotkey_error: state.hotkey_error.lock().unwrap().clone(),
    }
}

/// 숫자·글자 설정 (생각 수치, 컨테이너 크기, 단축키 등)
#[tauri::command]
fn set_value(app: AppHandle, state: State<AppState>, key: String, value: serde_json::Value) -> Result<(), String> {
    let a = &state.assistant;
    {
        let mut c = a.config.write().unwrap();
        match key.as_str() {
            "think_level" => c.think_level = value.as_u64().ok_or("숫자가 필요합니다")?.clamp(1, 5) as u8,
            "container_mb" => c.container_mb = value.as_u64().ok_or("숫자가 필요합니다")?.clamp(10, 10240),
            "memory_enabled" => c.memory_enabled = value.as_bool().ok_or("참/거짓이 필요합니다")?,
            "container_enabled" => c.container_enabled = value.as_bool().ok_or("참/거짓이 필요합니다")?,
            "hotkey_enabled" => c.hotkey_enabled = value.as_bool().ok_or("참/거짓이 필요합니다")?,
            "hotkey" => {
                let k = value.as_str().ok_or("글자가 필요합니다")?.trim().to_string();
                k.parse::<Shortcut>().map_err(|_| format!("'{k}'은(는) 단축키로 쓸 수 없습니다"))?;
                c.hotkey = k;
            }
            _ => return Err("모르는 설정입니다".into()),
        }
        c.save(&a.data_dir);
    }
    if key.starts_with("hotkey") {
        // 등록이 되는지 확인 (설정은 메인 창에서 바꾸니까 확인 후 다시 풀어 둠)
        register_hotkey(&app);
        let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
        if focused {
            pause_hotkey(&app);
        }
        if let Some(e) = state.hotkey_error.lock().unwrap().clone() {
            return Err(e);
        }
    }
    Ok(())
}

// ── 메모리 ──
#[tauri::command]
fn memory_list(state: State<AppState>) -> Vec<arcana_skills::memory::Fact> {
    state.assistant.memory.list()
}

#[tauri::command]
fn memory_add(state: State<AppState>, text: String) -> Result<arcana_skills::memory::Fact, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("내용을 입력해 주세요".into());
    }
    Ok(state.assistant.memory.add("note", t))
}

#[tauri::command]
fn memory_remove(state: State<AppState>, id: u64) {
    state.assistant.memory.remove(id)
}

#[tauri::command]
fn memory_clear(state: State<AppState>) {
    state.assistant.memory.clear()
}

// ── 컨테이너 메모리 ──
#[tauri::command]
fn container_info(state: State<AppState>) -> ContainerInfo {
    let a = &state.assistant;
    let c = a.config.read().unwrap();
    ContainerInfo {
        used_bytes: a.container.usage_bytes(),
        max_mb: c.container_mb,
        enabled: c.container_enabled,
        turns: a.container.recent(usize::MAX).len(),
    }
}

#[tauri::command]
fn container_clear(state: State<AppState>) {
    state.assistant.container.clear()
}

// ── 빠른 호출 (기본 Alt+Shift+A) ──
#[tauri::command]
fn quick_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window("quick") {
        let _ = w.hide();
    }
    // 닫았으니 다시 어디서든 부를 수 있게 (메인 창에서 입력 중이면 제외)
    let main_focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
    if !main_focused {
        register_hotkey(&app);
    }
}

/// 단축키는 키 입력을 가로채므로, Arcana 창에서 글을 쓰는 동안에는 잠깐 풀어 둠
/// 
fn pause_hotkey(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}

/// 마우스가 있는 화면 전체를 덮게 빠른 호출 창을 띄움
fn quick_show(app: &AppHandle) {
    let Some(w) = app.get_webview_window("quick") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.set_focus();
        return;
    }
    let monitor = w
        .cursor_position()
        .ok()
        .and_then(|p| w.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| w.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let _ = w.set_position(*m.position());
        let _ = w.set_size(*m.size());
    }
    let _ = w.emit("arcana://quick", "open");
    let _ = w.show();
    let _ = w.set_always_on_top(true);
    let _ = w.set_focus();
    pause_hotkey(app);
}

/// 설정의 단축키를 (다시) 등록
fn register_hotkey(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (key, on) = {
        let c = state.assistant.config.read().unwrap();
        (c.hotkey.clone(), c.hotkey_enabled)
    };
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut err = None;
    if on {
        match key.parse::<Shortcut>() {
            Ok(sc) => {
                if let Err(e) = gs.register(sc) {
                    err = Some(format!("단축키 {key}을(를) 등록하지 못했습니다. 다른 프로그램이 쓰고 있을 수 있습니다. ({e})"));
                }
            }
            Err(_) => err = Some(format!("'{key}'은(는) 단축키로 쓸 수 없습니다")),
        }
    }
    *state.hotkey_error.lock().unwrap() = err;
}

#[tauri::command]
fn set_option(state: State<AppState>, key: String, value: bool) {
    let a = &state.assistant;
    let mut c = a.config.write().unwrap();
    match key.as_str() {
        "show_debug" => c.show_debug = value,
        "auto_update" => c.auto_update = value,
        "memory_enabled" => c.memory_enabled = value,
        "container_enabled" => c.container_enabled = value,
        _ => return,
    }
    c.save(&a.data_dir);
}

/// 대답 속 링크 클릭 → 기본 브라우저로 열기 (http/https만)
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("웹 주소만 열 수 있습니다.".into());
    }
    open::that_detached(url).map_err(|e| e.to_string())
}

/// 설정 파일을 메모장 등으로 열기
#[tauri::command]
fn open_config(state: State<AppState>) -> Result<(), String> {
    let p = arcana_skills::Config::path(&state.assistant.data_dir);
    open::that_detached(p).map_err(|e| e.to_string())
}

/// 설정 파일 고친 뒤 다시 읽기
#[tauri::command]
fn reload_config(state: State<AppState>) {
    let a = &state.assistant;
    *a.config.write().unwrap() = arcana_skills::Config::load_or_create(&a.data_dir);
}

#[tauri::command]
async fn check_update(state: State<'_, AppState>) -> Result<updater::UpdateInfo, String> {
    if cfg!(debug_assertions) {
        return Err("개발 모드에서는 업데이트 안 함".into());
    }
    let info = tauri::async_runtime::spawn_blocking(updater::check)
        .await
        .map_err(|e| e.to_string())??;
    *state.update.lock().unwrap() = Some(info.clone());
    Ok(info)
}

#[tauri::command]
async fn apply_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let info = state
        .update
        .lock()
        .unwrap()
        .clone()
        .ok_or("먼저 업데이트 확인 필요")?;
    tauri::async_runtime::spawn_blocking(move || updater::apply(&info))
        .await
        .map_err(|e| e.to_string())??;
    updater::relaunch();
    app.exit(0);
    Ok(())
}

// ───────────────────── 시작 ─────────────────────

fn data_dir(app: &AppHandle) -> PathBuf {
    std::env::var_os("APPDATA")
        .map(|p| PathBuf::from(p).join("Arcana"))
        .unwrap_or_else(|| {
            app.path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("arcana-data"))
        })
}

fn main() {
    // 업데이트 직후 재시작이면 이전 프로세스가 완전히 꺼질 때까지 잠깐 대기
    if std::env::args().any(|a| a == updater::RESTART_FLAG) {
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }

    tauri::Builder::default()
        // 이미 켜져 있으면 새로 안 켜고 기존 창을 앞으로
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            let handle = app.handle().clone();
            let events: arcana_skills::EventSink = Arc::new(move |e: Event| {
                if let (Event::Alert { .. }, Some(w)) = (&e, handle.get_webview_window("main")) {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                let _ = handle.emit("arcana://event", e);
            });
            let brain = arcana_core::builtin::load().map_err(|e| e.to_string())?;
            let dir = data_dir(app.handle());
            // 대화 모델: 사용자가 학습한 것만 (%APPDATA%\\Arcana\\models). 없으면 대화는 검색·질문으로
            let talker = arcana_skills::load_talker(&dir)
                .ok()
                .flatten()
                .map(|t| Arc::new(t) as Arc<dyn arcana_core::Talker>);
            let assistant = Arc::new(Assistant::new(Box::new(brain), talker, dir, events));
            assistant.start_self_study();
            app.manage(AppState {
                assistant,
                update: Mutex::new(None),
                hotkey_error: Mutex::new(None),
            });
            // 빠른 호출 단축키 (기본 Alt+Shift+A, 설정에서 변경)
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, _sc, e| {
                        if e.state() == ShortcutState::Pressed {
                            quick_show(app);
                        }
                    })
                    .build(),
            )?;
            register_hotkey(app.handle());
            // 메인 창에 입력하는 동안은 단축키를 풀어 둠
            if let Some(main) = app.get_webview_window("main") {
                let h = app.handle().clone();
                main.on_window_event(move |e| {
                    if let tauri::WindowEvent::Focused(f) = e {
                        let quick_open = h.get_webview_window("quick").and_then(|w| w.is_visible().ok()).unwrap_or(false);
                        if *f {
                            pause_hotkey(&h);
                        } else if !quick_open {
                            register_hotkey(&h);
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send,
            study,
            study_status,
            open_url,
            feedback,
            confirm,
            info,
            set_option,
            open_config,
            reload_config,
            check_update,
            apply_update,
            set_value,
            memory_list,
            memory_add,
            memory_remove,
            memory_clear,
            container_info,
            container_clear,
            quick_hide
        ])
        .run(tauri::generate_context!())
        .expect("앱 실행 실패");
}

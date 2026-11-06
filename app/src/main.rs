// 배포 버전에서는 검은 콘솔 창 안 뜨게
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod updater;

use arcana_skills::{Assistant, Event, Reply, Response};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    assistant: Arc<Assistant>,
    update: Mutex<Option<updater::UpdateInfo>>,
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
    }
}

#[tauri::command]
fn set_option(state: State<AppState>, key: String, value: bool) {
    let a = &state.assistant;
    let mut c = a.config.write().unwrap();
    match key.as_str() {
        "show_debug" => c.show_debug = value,
        "auto_update" => c.auto_update = value,
        _ => return,
    }
    c.save(&a.data_dir);
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

/// 대화 모델 불러오기:
/// %APPDATA%\Arcana\models\arcana-chat.* 가 있으면 그걸 (그래픽카드로 학습한 큰 모델 등),
/// 없으면 exe에 내장된 기본 모델. 둘 다 실패하면 대화 없이 명령만 처리.
fn load_chat(dir: &std::path::Path) -> Option<arcana_core::ArcanaChat> {
    let m = dir.join("models");
    if let (Ok(w), Ok(j)) = (
        std::fs::read(m.join("arcana-chat.safetensors")),
        std::fs::read_to_string(m.join("arcana-chat.json")),
    ) {
        match arcana_core::ArcanaChat::from_bytes(&w, &j) {
            Ok(t) => return Some(t),
            Err(e) => eprintln!("외부 대화 모델 로드 실패, 내장 모델 사용: {e}"),
        }
    }
    arcana_core::builtin::load_chat().ok()
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
            let talker = load_chat(&dir).map(|t| Arc::new(t) as Arc<dyn arcana_core::Talker>);
            let assistant = Arc::new(Assistant::new(Box::new(brain), talker, dir, events));
            app.manage(AppState {
                assistant,
                update: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send,
            confirm,
            info,
            set_option,
            open_config,
            reload_config,
            check_update,
            apply_update
        ])
        .run(tauri::generate_context!())
        .expect("앱 실행 실패");
}

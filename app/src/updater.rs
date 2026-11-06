//! 자동 업데이트: 켤 때 GitHub 최신 릴리스를 확인하고, 새 버전이면 Arcana.exe를 받아서 바꿔 끼움.
//! 릴리스는 .github/workflows/release.yml 이 태그(v0.2.0 등)를 올리면 자동으로 만들어 줌.

use serde::Serialize;
use std::io::Read;

pub const REPO: &str = "XxErone2xX/Arcana";
const ASSET: &str = "Arcana.exe";
pub const RESTART_FLAG: &str = "--after-update";

#[derive(Serialize, Clone, Debug)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    #[serde(skip)]
    pub url: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(5))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent(concat!("Arcana-Updater/", env!("CARGO_PKG_VERSION")))
        .build()
}

pub fn check() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let v: serde_json::Value = agent()
        .get(&format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .call()
        .map_err(|e| format!("업데이트 확인 실패: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let tag = v["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches(['v', 'V'])
        .to_string();
    let url = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == ASSET))
        .and_then(|x| x["browser_download_url"].as_str())
        .map(String::from);
    let newer = match (
        semver::Version::parse(&tag),
        semver::Version::parse(&current),
    ) {
        (Ok(l), Ok(c)) => l > c,
        _ => false,
    };
    Ok(UpdateInfo {
        available: newer && url.is_some(),
        notes: v["body"].as_str().unwrap_or("").to_string(),
        latest: tag,
        current,
        url,
    })
}

/// 새 exe 받아서 지금 실행 중인 exe와 바꿔치기
pub fn apply(info: &UpdateInfo) -> Result<(), String> {
    let url = info.url.as_ref().ok_or("다운로드 주소 없음")?;
    let resp = agent()
        .get(url)
        .call()
        .map_err(|e| format!("다운로드 실패: {e}"))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(200 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() < 1024 * 100 || &bytes[..2] != b"MZ" {
        return Err("받은 파일이 올바른 exe가 아님".into());
    }
    let tmp = std::env::temp_dir().join(format!("Arcana-{}.exe", info.latest));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    self_replace::self_replace(&tmp).map_err(|e| format!("교체 실패: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

/// 업데이트 후 다시 켜기
pub fn relaunch() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg(RESTART_FLAG).spawn();
    }
}

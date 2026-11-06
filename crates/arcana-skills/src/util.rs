//! 여러 스킬이 같이 쓰는 도구들

/// 띄어쓰기 없애고 소문자로
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// 두 문자열이 얼마나 비슷한지 0.0 ~ 1.0 (편집 거리 기반)
pub fn similarity(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f32 / a.len().max(b.len()) as f32
}

/// 콘솔 창 안 띄우고 명령 실행
pub fn hidden_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// 가상 키 누르기 (볼륨 키 등). 윈도우가 아니면 아무것도 안 함.
pub fn press_key(vk: u8, times: usize) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
        for _ in 0..times {
            keybd_event(vk, 0, 0, 0);
            keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
        }
    }
    #[cfg(not(windows))]
    let _ = (vk, times);
}

/// 여러 키 동시에 누르기 (Win+PrtSc 등)
pub fn press_combo(vks: &[u8]) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
        for &k in vks {
            keybd_event(k, 0, 0, 0);
        }
        for &k in vks.iter().rev() {
            keybd_event(k, 0, KEYEVENTF_KEYUP, 0);
        }
    }
    #[cfg(not(windows))]
    let _ = vks;
}

#[cfg(test)]
mod tests {
    #[test]
    fn sim() {
        assert!(super::similarity("크롬창", "크롬") > 0.6);
        assert!(super::similarity("포토샵", "크롬") < 0.3);
    }
}

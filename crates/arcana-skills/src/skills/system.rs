//! 볼륨, 스크린샷, 화면 잠금, 전원

use crate::util::{hidden_command, press_combo, press_key};
use crate::{Context, Reply, Skill};
use arcana_core::Understanding;

const VK_VOLUME_MUTE: u8 = 0xAD;
const VK_VOLUME_DOWN: u8 = 0xAE;
const VK_VOLUME_UP: u8 = 0xAF;
const VK_LWIN: u8 = 0x5B;
const VK_SNAPSHOT: u8 = 0x2C;

pub struct Volume;

impl Skill for Volume {
    fn intents(&self) -> &[&'static str] {
        &["volume_up", "volume_down", "mute"]
    }
    fn describe(&self) -> &'static str {
        "볼륨 — \"소리 키워\", \"볼륨 줄여\", \"음소거\""
    }
    fn run(&self, intent: &str, _: &Understanding, _: &Context) -> Reply {
        match intent {
            "volume_up" => {
                press_key(VK_VOLUME_UP, 5);
                Reply::say("소리 키웠어.")
            }
            "volume_down" => {
                press_key(VK_VOLUME_DOWN, 5);
                Reply::say("소리 줄였어.")
            }
            _ => {
                press_key(VK_VOLUME_MUTE, 1);
                Reply::say("음소거 전환했어.")
            }
        }
    }
}

pub struct Screen;

impl Skill for Screen {
    fn intents(&self) -> &[&'static str] {
        &["screenshot", "lock"]
    }
    fn describe(&self) -> &'static str {
        "화면 — \"스샷\", \"컴 잠가\""
    }
    fn run(&self, intent: &str, _: &Understanding, _: &Context) -> Reply {
        if intent == "screenshot" {
            press_combo(&[VK_LWIN, VK_SNAPSHOT]);
            Reply::say("캡처했어. (사진 → 스크린샷 폴더)")
        } else {
            #[cfg(windows)]
            unsafe {
                windows_sys::Win32::System::Shutdown::LockWorkStation();
            }
            Reply::say("화면 잠갔어.")
        }
    }
}

pub struct Power;

impl Skill for Power {
    fn intents(&self) -> &[&'static str] {
        &["shutdown"]
    }
    fn describe(&self) -> &'static str {
        "전원 — \"컴퓨터 꺼줘\" (끄기 전에 물어봄)"
    }
    fn run(&self, _: &str, _: &Understanding, _: &Context) -> Reply {
        Reply::ask("진짜 컴퓨터 끌까?")
    }
    fn confirmed(&self, _: &str, _: &Context) -> Reply {
        match hidden_command("shutdown").args(["/s", "/t", "10"]).spawn() {
            Ok(_) => Reply::say("10초 뒤에 꺼져. 취소하려면 cmd에 shutdown /a 입력해."),
            Err(e) => Reply::error(format!("종료 실패: {e}")),
        }
    }
}

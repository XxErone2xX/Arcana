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
                Reply::say("볼륨을 높였습니다.")
            }
            "volume_down" => {
                press_key(VK_VOLUME_DOWN, 5);
                Reply::say("볼륨을 낮췄습니다.")
            }
            _ => {
                press_key(VK_VOLUME_MUTE, 1);
                Reply::say("음소거를 전환했습니다.")
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
            Reply::say("화면을 캡처했습니다. (사진 → 스크린샷 폴더에 저장됨)")
        } else {
            #[cfg(windows)]
            unsafe {
                windows_sys::Win32::System::Shutdown::LockWorkStation();
            }
            Reply::say("화면을 잠갔습니다.")
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
        Reply::ask("컴퓨터를 종료할까요?")
    }
    fn confirmed(&self, _: &str, _: &Context) -> Reply {
        match hidden_command("shutdown").args(["/s", "/t", "10"]).spawn() {
            Ok(_) => {
                Reply::say("10초 뒤에 종료됩니다. 취소하시려면 cmd에 shutdown /a 를 입력하십시오.")
            }
            Err(e) => Reply::error(format!("종료하지 못했습니다. ({e})")),
        }
    }
}

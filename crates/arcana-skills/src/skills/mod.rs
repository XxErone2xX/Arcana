//! 내장 스킬 목록. 새 스킬 만들면 여기 builtin()에 추가.

mod apps;
mod chat;
mod clock;
mod knowledge;
mod memo;
mod system;
mod timer;
pub mod web;

use crate::Skill;

pub fn builtin() -> Vec<Box<dyn Skill>> {
    vec![
        Box::new(apps::Apps),
        Box::new(timer::Timer),
        Box::new(clock::Clock),
        Box::new(web::Web),
        Box::new(system::Volume),
        Box::new(system::Screen),
        Box::new(system::Power),
        Box::new(memo::Memo),
        Box::new(knowledge::Knowledge),
        Box::new(chat::Chat),
    ]
}

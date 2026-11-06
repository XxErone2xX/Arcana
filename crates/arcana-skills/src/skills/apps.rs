//! 프로그램 켜기/끄기. 앱 목록은 config.json의 "apps"에서 관리 (재학습 필요 없음).

use crate::util::{eul, eun, hidden_command, iga, normalize, similarity};
use crate::{Context, Reply, Skill};
use arcana_core::Understanding;

pub struct Apps;

/// 사용자가 말한 이름 → config의 앱 이름
fn find(cx: &Context, said: &str) -> Option<(String, crate::config::AppEntry)> {
    let cfg = cx.cfg();
    let n = normalize(said);
    let n = n.trim_end_matches(['을', '를', '좀']);
    let mut best: Option<(f32, &String)> = None;
    for (name, e) in &cfg.apps {
        for cand in std::iter::once(name).chain(e.aliases.iter()) {
            let c = normalize(cand);
            let score = if c == n {
                1.0
            } else if n.contains(&c) && c.chars().count() >= 2 {
                0.8 // "크롬창" ⊃ "크롬"
            } else {
                similarity(&c, n)
            };
            if score >= 0.5 && best.map_or(true, |(s, _)| score > s) {
                best = Some((score, name));
            }
        }
    }
    best.map(|(_, name)| (name.clone(), cfg.apps[name].clone()))
}

impl Skill for Apps {
    fn intents(&self) -> &[&'static str] {
        &["open_app", "close_app"]
    }

    fn describe(&self) -> &'static str {
        "프로그램 켜기/끄기 — \"디코 켜줘\", \"크롬 꺼\""
    }

    fn run(&self, intent: &str, u: &Understanding, cx: &Context) -> Reply {
        let said = u.slots.get("APP").cloned().unwrap_or_default();
        if said.is_empty() {
            return Reply::error("어떤 프로그램인지 잘 모르겠습니다.");
        }
        let Some((name, app)) = find(cx, &said) else {
            return Reply::error(format!(
                "'{said}'{}은 아직 등록되지 않은 프로그램입니다. 메뉴 → 설정 파일 열기에서 추가해 주십시오.", eun(&said)
            ));
        };
        if intent == "open_app" {
            match open::that_detached(&app.open) {
                Ok(_) => Reply::say(format!("{name}{} 실행합니다.", eul(&name))),
                Err(e) => Reply::error(format!(
                    "{name} 실행에 실패했습니다. 설정 파일에서 경로를 확인해 주십시오. ({e})"
                )),
            }
        } else {
            if app.close.is_empty() {
                return Reply::error(format!(
                    "{name}{} 끄면 문제가 생길 수 있어서 종료하지 않겠습니다.",
                    eun(&name)
                ));
            }
            let mut killed = false;
            for p in &app.close {
                if let Ok(o) = hidden_command("taskkill").args(["/IM", p, "/F"]).output() {
                    killed |= o.status.success();
                }
            }
            if killed {
                Reply::say(format!("{name}{} 종료했습니다.", eul(&name)))
            } else {
                Reply::say(format!("{name}{} 실행 중이 아닌 것 같습니다.", iga(&name)))
            }
        }
    }
}

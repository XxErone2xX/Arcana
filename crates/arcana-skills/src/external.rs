//! 외부 AI API (선택): OpenAI 호환 · Anthropic(Claude) · Google Gemini
//!
//! - 명령(앱 열기, 타이머 등)은 언제나 Arcana가 직접 처리하고, 대화만 외부 AI에게 맡김
//! - api_mode "fallback": Arcana가 모르는 말이거나 자연스럽게 대답할 자신이 없을 때만
//!   api_mode "always": 잡담은 전부 외부 AI가
//! - 외부 AI의 대답은 Arcana 학습 재료로 쓰지 않음 (👍/👎·가르치기 저장 안 함, 대화 모델 학습에 안 들어감)
//! - "OpenAI 호환"은 주소만 바꾸면 OpenRouter, Groq, DeepSeek, Ollama·LM Studio(내 PC) 등도 됨

use crate::config::Config;
use crate::context::Turn;
use serde_json::{json, Value};
use std::time::Duration;

pub const PROVIDERS: &[(&str, &str)] = &[
    ("", "사용 안 함"),
    ("openai", "OpenAI 호환 (ChatGPT, OpenRouter, Groq, Ollama 등)"),
    ("anthropic", "Anthropic (Claude)"),
    ("gemini", "Google Gemini"),
];

pub fn enabled(c: &Config) -> bool {
    !c.api_provider.is_empty() && (!c.api_key.trim().is_empty() || is_local(&base_url(c))) && !c.api_model.trim().is_empty()
}

/// 대화창에 붙는 출처 표시 ("Claude · claude-haiku-5-5")
pub fn label(c: &Config) -> String {
    let p = match c.api_provider.as_str() {
        "anthropic" => "Claude",
        "gemini" => "Gemini",
        _ => {
            let b = base_url(c);
            if b.contains("openrouter") {
                "OpenRouter"
            } else if b.contains("groq") {
                "Groq"
            } else if is_local(&b) {
                "내 PC"
            } else if b.contains("openai.com") {
                "OpenAI"
            } else {
                "외부 AI"
            }
        }
    };
    format!("{p} · {}", c.api_model.trim())
}

fn base_url(c: &Config) -> String {
    let b = c.api_base.trim().trim_end_matches('/');
    if !b.is_empty() {
        return b.to_string();
    }
    match c.api_provider.as_str() {
        "anthropic" => "https://api.anthropic.com/v1".into(),
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".into(),
        _ => "https://api.openai.com/v1".into(),
    }
}

fn is_local(url: &str) -> bool {
    let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default();
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host.starts_with("192.168.") || host.starts_with("10.")
}

fn system_prompt(c: &Config, user_name: Option<&str>) -> String {
    let mut s = format!(
        "너는 '{}'라는 이름의 PC 비서다. 사용자의 Windows PC에서 동작한다. \
         항상 한국어 합니다체(\"~합니다\", \"~습니까?\")로, 짧고 정확하게(보통 1~4문장) 대답한다. \
         모르는 것은 지어내지 말고 모른다고 말한다. 마크다운 표·제목은 쓰지 말고, 목록이 꼭 필요할 때만 짧게 쓴다. \
         앱 열기·끄기, 타이머, 메모, 검색 같은 PC 명령은 앱이 따로 처리하므로, 그런 걸 실행했다고 거짓말하지 않는다.",
        c.name
    );
    if let Some(n) = user_name {
        s.push_str(&format!(" 사용자의 이름은 {n}이다."));
    }
    s
}

/// 외부 AI에게 묻기. history: 최근 대화 (오래된 것 → 최근)
pub fn ask(c: &Config, history: &[Turn], text: &str, user_name: Option<&str>) -> Result<String, String> {
    let base = base_url(c);
    let model = c.api_model.trim();
    if model.is_empty() {
        return Err("모델 이름을 설정에서 넣어 주십시오.".into());
    }
    let key = c.api_key.trim();
    let sys = system_prompt(c, user_name);
    // 최근 대화 (최대 6번 주고받은 것, 너무 긴 건 자름)
    let cut = |t: &str| t.chars().take(800).collect::<String>();
    let turns: Vec<(&str, String)> = history
        .iter()
        .rev()
        .take(6)
        .rev()
        .flat_map(|t| [("user", cut(&t.user)), ("assistant", cut(&t.reply))])
        .filter(|(_, x)| !x.trim().is_empty())
        .collect();

    let (url, headers, body): (String, Vec<(&str, String)>, Value) = match c.api_provider.as_str() {
        "anthropic" => {
            let mut msgs: Vec<Value> = turns.iter().map(|(r, t)| json!({"role": r, "content": t})).collect();
            msgs.push(json!({"role": "user", "content": text}));
            (
                format!("{base}/messages"),
                vec![("x-api-key", key.to_string()), ("anthropic-version", "2023-06-01".into())],
                json!({"model": model, "max_tokens": 1024, "system": sys, "messages": msgs}),
            )
        }
        "gemini" => {
            let mut contents: Vec<Value> = turns
                .iter()
                .map(|(r, t)| json!({"role": if *r == "assistant" { "model" } else { "user" }, "parts": [{"text": t}]}))
                .collect();
            contents.push(json!({"role": "user", "parts": [{"text": text}]}));
            (
                format!("{base}/models/{model}:generateContent"),
                vec![("x-goog-api-key", key.to_string())],
                json!({"systemInstruction": {"parts": [{"text": sys}]}, "contents": contents}),
            )
        }
        _ => {
            let mut msgs = vec![json!({"role": "system", "content": sys})];
            msgs.extend(turns.iter().map(|(r, t)| json!({"role": r, "content": t})));
            msgs.push(json!({"role": "user", "content": text}));
            let mut h = vec![];
            if !key.is_empty() {
                h.push(("Authorization", format!("Bearer {key}")));
            }
            (format!("{base}/chat/completions"), h, json!({"model": model, "messages": msgs}))
        }
    };
    let hdr: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let timeout = Duration::from_secs(90);
    let res = if is_local(&url) {
        // 내 PC(Ollama, LM Studio)는 프록시를 거치지 않고 바로
        let mut r = ureq::AgentBuilder::new().timeout(timeout).build().post(&url);
        for (k, v) in &hdr {
            r = r.set(k, v);
        }
        r.send_json(body)
    } else {
        crate::net::post_json(&url, &hdr, &body, timeout)
    };
    let v: Value = match res {
        Ok(r) => r.into_json().map_err(|e| format!("대답을 읽지 못했습니다 ({e})"))?,
        Err(ureq::Error::Status(code, r)) => {
            let detail = r
                .into_json::<Value>()
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .or_else(|| v.pointer("/error"))
                        .or_else(|| v.pointer("/message"))
                        .and_then(|m| m.as_str().map(String::from))
                })
                .unwrap_or_default();
            let why = match code {
                401 | 403 => "API 키가 맞지 않거나 권한이 없습니다",
                404 => "모델 이름이나 주소를 찾을 수 없습니다",
                429 => "사용 한도를 넘었거나 잔액이 부족합니다",
                400 => "요청 형식이 맞지 않습니다 (모델 이름을 확인해 주십시오)",
                c if c >= 500 => "AI 서버에 문제가 있습니다. 잠시 뒤 다시 시도해 주십시오",
                _ => "요청이 거절되었습니다",
            };
            let detail: String = detail.chars().take(160).collect();
            return Err(if detail.is_empty() { format!("{why} (HTTP {code})") } else { format!("{why} (HTTP {code}: {detail})") });
        }
        Err(e) => return Err(crate::net::describe(&e)),
    };
    let out = match c.api_provider.as_str() {
        "anthropic" => v["content"]
            .as_array()
            .map(|a| a.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("")),
        "gemini" => v.pointer("/candidates/0/content/parts").and_then(|p| p.as_array()).map(|a| {
            a.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("")
        }),
        _ => v.pointer("/choices/0/message/content").and_then(|t| t.as_str()).map(String::from),
    }
    .unwrap_or_default();
    let out = out.trim().to_string();
    if out.is_empty() {
        return Err("AI가 빈 대답을 보냈습니다 (안전 필터에 걸렸을 수 있습니다)".into());
    }
    Ok(out)
}

/// 설정 화면의 "연결 시험"
pub fn test(c: &Config) -> Result<String, String> {
    if c.api_provider.is_empty() {
        return Err("먼저 사용할 AI를 고르십시오.".into());
    }
    if c.api_key.trim().is_empty() && !is_local(&base_url(c)) {
        return Err("API 키를 넣어 주십시오.".into());
    }
    let r = ask(c, &[], "연결 시험입니다. '연결되었습니다.'라고만 대답해 주십시오.", None)?;
    Ok(format!("{} 연결 성공: {}", label(c), r.chars().take(60).collect::<String>()))
}

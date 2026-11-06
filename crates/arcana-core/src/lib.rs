//! # arcana-core — Arcana 모델 (두뇌)
//!
//! Arcana = 처음부터 직접 만든 모델 이름. 두 부분으로 되어 있음:
//! - [`ArcanaNlu`]  명령 이해 (어떤 명령인지 + 앱 이름/시간 등 뽑기)
//! - [`ArcanaChat`] 대화 (대답을 글자 하나씩 직접 만들어냄)
//!
//! 문장 하나를 받아서 "무슨 뜻인지"를 돌려준다.
//! ```text
//! "라면 3분 타이머 맞춰줘"  →  Understanding { intent: "set_timer", confidence: 0.96, slots: {TIME: "3분"} }
//! ```
//!
//! 나중에 두뇌를 바꾸고 싶으면 (더 큰 모델, 생성형 모델 등) [`Brain`] 트레이트만 구현하면
//! 앱이나 스킬 쪽 코드는 안 건드려도 된다.

pub mod gpt;
pub mod model;
pub mod safetensors;
pub mod tensor;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum Error {
    Model(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Model(m) => write!(f, "모델 오류: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// 문장을 이해한 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Understanding {
    pub intent: String,
    pub confidence: f32,
    /// 슬롯 이름 → 값  (예: "APP" → "디스코드")
    pub slots: BTreeMap<String, String>,
}

/// 두뇌 인터페이스. 지금은 [`ArcanaNlu`] 하나지만 나중에 다른 두뇌를 끼울 수 있음.
pub trait Brain: Send + Sync {
    fn understand(&self, text: &str) -> Understanding;
    /// 이 두뇌가 아는 의도 목록
    fn intents(&self) -> &[String];
    fn name(&self) -> String;
}

#[derive(Deserialize)]
struct Meta {
    format_version: u32,
    architecture: String,
    config: model::Config,
    layer_norm_eps: f32,
    vocab: Vec<String>,
    intents: Vec<String>,
    tags: Vec<String>,
}

/// Arcana 명령 이해 모델 — 처음부터 학습시킨 의도+슬롯 모델 (JointBERT 구조 참고)
pub struct ArcanaNlu {
    encoder: model::Encoder,
    vocab: std::collections::HashMap<char, usize>,
    intents: Vec<String>,
    tags: Vec<String>,
    arch: String,
}

pub const SUPPORTED_FORMAT: u32 = 1;

impl ArcanaNlu {
    /// `weights` = arcana-nlu.safetensors 내용, `meta_json` = arcana-nlu.json 내용
    pub fn from_bytes(weights: &[u8], meta_json: &str) -> Result<Self, Error> {
        let meta: Meta =
            serde_json::from_str(meta_json).map_err(|e| Error::Model(format!("json: {e}")))?;
        if meta.format_version > SUPPORTED_FORMAT {
            return Err(Error::Model(format!(
                "모델 형식 v{}는 이 버전 Arcana가 못 읽음 (앱 업데이트 필요)",
                meta.format_version
            )));
        }
        if meta.architecture != "joint-encoder-v1" {
            return Err(Error::Model(format!("모르는 구조: {}", meta.architecture)));
        }
        let tensors = safetensors::Tensors::parse(weights)?;
        let encoder = model::Encoder::load(meta.config, &tensors, meta.layer_norm_eps)?;
        let mut vocab = std::collections::HashMap::new();
        for (i, tok) in meta.vocab.iter().enumerate() {
            let mut cs = tok.chars();
            if let (Some(c), None) = (cs.next(), cs.next()) {
                vocab.insert(c, i); // 한 글자짜리만 (특수 토큰 <pad> 등은 제외)
            }
        }
        Ok(Self {
            encoder,
            vocab,
            intents: meta.intents,
            tags: meta.tags,
            arch: meta.architecture,
        })
    }

    const UNK: usize = 1;
    const CLS: usize = 2;

    /// 파이썬 토크나이저와 같은 규칙: 소문자화 → [CLS] + 글자들
    pub fn encode(&self, text: &str) -> (Vec<usize>, Vec<char>) {
        let chars: Vec<char> = lower(text)
            .chars()
            .take(self.encoder.cfg.max_len - 1)
            .collect();
        let mut ids = vec![Self::CLS];
        ids.extend(
            chars
                .iter()
                .map(|c| *self.vocab.get(c).unwrap_or(&Self::UNK)),
        );
        (ids, chars)
    }

    /// 테스트/디버그용: 원본 출력
    pub fn raw(&self, text: &str) -> model::Output {
        self.encoder.forward(&self.encode(text).0)
    }
}

/// 파이썬 str.lower() 와 같게 (글자 수가 바뀌는 특수문자는 그대로 둠)
fn lower(s: &str) -> String {
    s.chars()
        .map(|c| {
            let mut l = c.to_lowercase();
            match (l.next(), l.next()) {
                (Some(x), None) => x,
                _ => c,
            }
        })
        .collect()
}

impl Brain for ArcanaNlu {
    fn understand(&self, text: &str) -> Understanding {
        let (ids, chars) = self.encode(text);
        let out = self.encoder.forward(&ids);
        let best = tensor::argmax(&out.intent_probs);

        // BIO 태그 → 슬롯 값 (첫 번째로 나온 값만 사용)
        let mut slots = BTreeMap::new();
        let mut cur: Option<(String, String)> = None;
        let flush = |cur: &mut Option<(String, String)>, slots: &mut BTreeMap<String, String>| {
            if let Some((name, val)) = cur.take() {
                slots.entry(name).or_insert_with(|| val.trim().to_string());
            }
        };
        for (i, ch) in chars.iter().enumerate() {
            let tag = &self.tags[tensor::argmax(out.slot_logits.row(i + 1))];
            if let Some(name) = tag.strip_prefix("B-") {
                flush(&mut cur, &mut slots);
                cur = Some((name.to_string(), ch.to_string()));
            } else if let Some(name) = tag.strip_prefix("I-") {
                match &mut cur {
                    Some((n, v)) if n == name => v.push(*ch),
                    _ => flush(&mut cur, &mut slots),
                }
            } else {
                flush(&mut cur, &mut slots);
            }
        }
        flush(&mut cur, &mut slots);

        Understanding {
            intent: self.intents[best].clone(),
            confidence: out.intent_probs[best],
            slots,
        }
    }

    fn intents(&self) -> &[String] {
        &self.intents
    }

    fn name(&self) -> String {
        format!(
            "Arcana 명령 이해 v1 · {}층 {}차원 ({})",
            self.encoder.cfg.n_layers, self.encoder.cfg.d_model, self.arch
        )
    }
}

// ───────────────────────── 대화 (생성형) ─────────────────────────

/// 대화 인터페이스. 지금은 [`ArcanaChat`] 하나지만, 나중에 더 큰 모델로 바꿔 끼울 수 있음.
pub trait Talker: Send + Sync {
    fn reply(&self, text: &str) -> String;
    fn name(&self) -> String;
}

#[derive(Deserialize)]
struct ChatMeta {
    format_version: u32,
    architecture: String,
    config: gpt::GptConfig,
    layer_norm_eps: f32,
    vocab: Vec<String>,
}

/// Arcana 대화 모델 — 처음부터 학습시킨 글자 단위 생성 모델 (GPT 계열 디코더 구조 참고)
pub struct ArcanaChat {
    gpt: gpt::Gpt,
    vocab: std::collections::HashMap<char, usize>,
    itos: Vec<String>,
    pub temperature: f32,
    pub top_k: usize,
    rng: std::sync::Mutex<gpt::Rng>,
}

impl ArcanaChat {
    const UNK: usize = 1;
    const USER: usize = 2;
    const BOT: usize = 3;
    const EOS: usize = 4;
    const N_SPECIAL: usize = 5;

    pub fn from_bytes(weights: &[u8], meta_json: &str) -> Result<Self, Error> {
        let meta: ChatMeta =
            serde_json::from_str(meta_json).map_err(|e| Error::Model(format!("json: {e}")))?;
        if meta.format_version > SUPPORTED_FORMAT || meta.architecture != "arcana-chat-v1" {
            return Err(Error::Model(format!(
                "모르는 대화 모델 형식: {} v{}",
                meta.architecture, meta.format_version
            )));
        }
        let gpt = gpt::Gpt::load(
            meta.config,
            &safetensors::Tensors::parse(weights)?,
            meta.layer_norm_eps,
        )?;
        let mut vocab = std::collections::HashMap::new();
        for (i, t) in meta.vocab.iter().enumerate().skip(Self::N_SPECIAL) {
            if let Some(c) = t.chars().next() {
                vocab.insert(c, i);
            }
        }
        Ok(Self {
            gpt,
            vocab,
            itos: meta.vocab,
            temperature: 0.7,
            top_k: 8,
            rng: std::sync::Mutex::new(gpt::Rng::seeded_by_time()),
        })
    }

    /// temperature 0 = 항상 같은 대답 (테스트용)
    pub fn generate(&self, text: &str, temperature: f32, rng: &mut gpt::Rng) -> String {
        let max_len = self.gpt.cfg.max_len;
        let mut prompt = vec![Self::USER];
        prompt.extend(
            lower(text)
                .chars()
                .take(max_len / 2)
                .map(|c| *self.vocab.get(&c).unwrap_or(&Self::UNK)),
        );
        prompt.push(Self::BOT);

        let mut cache = self.gpt.new_cache();
        let mut logits = Vec::new();
        for &t in &prompt {
            logits = self.gpt.step(t, &mut cache);
        }
        let mut out = String::new();
        for _ in 0..80 {
            if cache.len >= max_len {
                break;
            }
            let next = gpt::sample(&logits, temperature, self.top_k, rng);
            if next == Self::EOS {
                break;
            }
            if next >= Self::N_SPECIAL {
                out.push_str(&self.itos[next]);
            }
            logits = self.gpt.step(next, &mut cache);
        }
        out
    }
}

impl Talker for ArcanaChat {
    fn reply(&self, text: &str) -> String {
        let mut rng = self.rng.lock().unwrap();
        self.generate(text, self.temperature, &mut rng)
    }

    fn name(&self) -> String {
        let c = &self.gpt.cfg;
        format!("Arcana 대화 v1 · {}층 {}차원", c.n_layers, c.d_model)
    }
}

/// 앱에 내장된 기본 모델들
pub mod builtin {
    pub const WEIGHTS: &[u8] = include_bytes!("../../../models/arcana-nlu.safetensors");
    pub const META: &str = include_str!("../../../models/arcana-nlu.json");
    pub const CHAT_WEIGHTS: &[u8] = include_bytes!("../../../models/arcana-chat.safetensors");
    pub const CHAT_META: &str = include_str!("../../../models/arcana-chat.json");

    pub fn load() -> Result<super::ArcanaNlu, super::Error> {
        super::ArcanaNlu::from_bytes(WEIGHTS, META)
    }

    pub fn load_chat() -> Result<super::ArcanaChat, super::Error> {
        super::ArcanaChat::from_bytes(CHAT_WEIGHTS, CHAT_META)
    }
}

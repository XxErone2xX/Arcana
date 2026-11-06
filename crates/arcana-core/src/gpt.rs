//! Arcana 대화 모델 추론 (training/chat_model.py 와 같은 계산)
//! 구조는 GPT 계열 디코더를 참고했고, 코드와 가중치는 전부 직접 만든 것
//! 디코더 전용 트랜스포머 + KV 캐시 (이미 계산한 글자는 다시 계산 안 함)

use crate::safetensors::Tensors;
use crate::tensor::{dot, gelu_inplace, softmax_inplace, LayerNorm, Linear, Mat};
use crate::Error;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct GptConfig {
    pub vocab_size: usize,
    pub d_model: usize,
    pub n_heads: usize,
    pub n_layers: usize,
    pub d_ff: usize,
    pub max_len: usize,
}

struct Block {
    ln1: LayerNorm,
    qkv: Linear,
    out: Linear,
    ln2: LayerNorm,
    ff1: Linear,
    ff2: Linear,
}

pub struct Gpt {
    pub cfg: GptConfig,
    tok_emb: Mat,
    pos_emb: Mat,
    blocks: Vec<Block>,
    ln_f: LayerNorm,
    lm_head: Linear,
}

/// 층마다 지금까지의 K, V 저장
pub struct KvCache {
    k: Vec<Vec<f32>>, // [층][위치 * d]
    v: Vec<Vec<f32>>,
    pub len: usize,
}

impl Gpt {
    pub fn load(cfg: GptConfig, w: &Tensors, eps: f32) -> Result<Self, Error> {
        let lin = |p: &str| -> Result<Linear, Error> {
            Ok(Linear {
                weight: w.mat(&format!("{p}.weight"))?,
                bias: w.vec(&format!("{p}.bias"))?,
            })
        };
        let ln = |p: &str| -> Result<LayerNorm, Error> {
            Ok(LayerNorm {
                weight: w.vec(&format!("{p}.weight"))?,
                bias: w.vec(&format!("{p}.bias"))?,
                eps,
            })
        };
        let mut blocks = Vec::new();
        for i in 0..cfg.n_layers {
            let p = format!("blocks.{i}");
            blocks.push(Block {
                ln1: ln(&format!("{p}.ln1"))?,
                qkv: lin(&format!("{p}.attn.qkv"))?,
                out: lin(&format!("{p}.attn.out"))?,
                ln2: ln(&format!("{p}.ln2"))?,
                ff1: lin(&format!("{p}.ff.0"))?,
                ff2: lin(&format!("{p}.ff.2"))?,
            });
        }
        Ok(Self {
            tok_emb: w.mat("tok_emb.weight")?,
            pos_emb: w.mat("pos_emb.weight")?,
            blocks,
            ln_f: ln("ln_f")?,
            lm_head: lin("lm_head")?,
            cfg,
        })
    }

    pub fn new_cache(&self) -> KvCache {
        KvCache {
            k: vec![Vec::new(); self.cfg.n_layers],
            v: vec![Vec::new(); self.cfg.n_layers],
            len: 0,
        }
    }

    /// 글자 하나를 넣고 다음 글자 점수(logits)를 돌려줌
    pub fn step(&self, token: usize, cache: &mut KvCache) -> Vec<f32> {
        let d = self.cfg.d_model;
        let h = self.cfg.n_heads;
        let dk = d / h;
        let pos = cache.len.min(self.cfg.max_len - 1);
        let e = self.tok_emb.row(token.min(self.cfg.vocab_size - 1));
        let p = self.pos_emb.row(pos);
        let mut x = Mat {
            rows: 1,
            cols: d,
            data: e.iter().zip(p).map(|(a, b)| a + b).collect(),
        };

        let scale = 1.0 / (dk as f32).sqrt();
        for (li, b) in self.blocks.iter().enumerate() {
            let qkv = b.qkv.forward(&b.ln1.forward(&x)).data; // [3d]
            cache.k[li].extend_from_slice(&qkv[d..2 * d]);
            cache.v[li].extend_from_slice(&qkv[2 * d..]);
            let t_len = cache.len + 1;
            let mut y = vec![0.0f32; d];
            let mut scores = vec![0.0f32; t_len];
            for head in 0..h {
                let o = head * dk;
                let q = &qkv[o..o + dk];
                for j in 0..t_len {
                    scores[j] = dot(q, &cache.k[li][j * d + o..j * d + o + dk]) * scale;
                }
                softmax_inplace(&mut scores);
                for j in 0..t_len {
                    let v = &cache.v[li][j * d + o..j * d + o + dk];
                    for k in 0..dk {
                        y[o + k] += scores[j] * v[k];
                    }
                }
            }
            x.add_assign(&b.out.forward(&Mat {
                rows: 1,
                cols: d,
                data: y,
            }));
            let mut hdn = b.ff1.forward(&b.ln2.forward(&x));
            gelu_inplace(&mut hdn);
            x.add_assign(&b.ff2.forward(&hdn));
        }
        cache.len += 1;
        self.lm_head.forward(&self.ln_f.forward(&x)).data
    }
}

/// 아주 단순한 난수 생성기 (외부 라이브러리 없이)
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15 | 1)
    }
    pub fn seeded_by_time() -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        Self::new(n)
    }
    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// 다음 글자 고르기. temperature 0이면 항상 1등 (테스트용)
pub fn sample(logits: &[f32], temperature: f32, top_k: usize, rng: &mut Rng) -> usize {
    if temperature <= 0.0 {
        return crate::tensor::argmax(logits);
    }
    let mut idx: Vec<usize> = (0..logits.len()).collect();
    idx.sort_unstable_by(|&a, &b| {
        logits[b]
            .partial_cmp(&logits[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    idx.truncate(top_k.max(1));
    let mut probs: Vec<f32> = idx.iter().map(|&i| logits[i] / temperature).collect();
    softmax_inplace(&mut probs);
    let mut r = rng.next_f32();
    for (k, p) in probs.iter().enumerate() {
        if r < *p {
            return idx[k];
        }
        r -= p;
    }
    idx[0]
}

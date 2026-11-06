//! 트랜스포머 인코더 (training/model.py 와 똑같은 계산을 Rust로 구현)
//! 구조: 글자 임베딩 + 위치 임베딩 → Pre-LN 인코더 블록 × N → [CLS]로 의도, 각 글자로 슬롯

use crate::safetensors::Tensors;
use crate::tensor::{gelu_inplace, softmax_inplace, LayerNorm, Linear, Mat};
use crate::Error;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub vocab_size: usize,
    pub n_intents: usize,
    pub n_tags: usize,
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

pub struct Encoder {
    pub cfg: Config,
    tok_emb: Mat,
    pos_emb: Mat,
    blocks: Vec<Block>,
    ln_f: LayerNorm,
    intent_head: Linear,
    slot_head: Linear,
}

pub struct Output {
    /// 의도별 확률 (softmax 적용됨)
    pub intent_probs: Vec<f32>,
    /// 글자별 슬롯 점수 [[CLS] 포함 길이, n_tags]
    pub slot_logits: Mat,
}

impl Encoder {
    pub fn load(cfg: Config, w: &Tensors, eps: f32) -> Result<Self, Error> {
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
            intent_head: lin("intent_head")?,
            slot_head: lin("slot_head")?,
            cfg,
        })
    }

    pub fn forward(&self, ids: &[usize]) -> Output {
        let d = self.cfg.d_model;
        let t_len = ids.len();
        // 임베딩
        let mut x = Mat::zeros(t_len, d);
        for (t, &id) in ids.iter().enumerate() {
            let e = self.tok_emb.row(id.min(self.cfg.vocab_size - 1));
            let p = self.pos_emb.row(t);
            for (k, v) in x.row_mut(t).iter_mut().enumerate() {
                *v = e[k] + p[k];
            }
        }
        // 인코더 블록들
        for b in &self.blocks {
            let a = self.attention(b, &b.ln1.forward(&x));
            x.add_assign(&a);
            let mut h = b.ff1.forward(&b.ln2.forward(&x));
            gelu_inplace(&mut h);
            x.add_assign(&b.ff2.forward(&h));
        }
        let x = self.ln_f.forward(&x);

        let cls = Mat {
            rows: 1,
            cols: d,
            data: x.row(0).to_vec(),
        };
        let mut intent_probs = self.intent_head.forward(&cls).data;
        softmax_inplace(&mut intent_probs);
        Output {
            intent_probs,
            slot_logits: self.slot_head.forward(&x),
        }
    }

    /// 멀티헤드 셀프 어텐션: softmax(QKᵀ/√dk)·V
    fn attention(&self, b: &Block, x: &Mat) -> Mat {
        let (t_len, d, h) = (x.rows, self.cfg.d_model, self.cfg.n_heads);
        let dk = d / h;
        let scale = 1.0 / (dk as f32).sqrt();
        let qkv = b.qkv.forward(x); // [T, 3d] = q | k | v
        let mut y = Mat::zeros(t_len, d);
        let mut scores = vec![0.0f32; t_len];
        for head in 0..h {
            let (qo, ko, vo) = (head * dk, d + head * dk, 2 * d + head * dk);
            for i in 0..t_len {
                let q = &qkv.row(i)[qo..qo + dk];
                for j in 0..t_len {
                    scores[j] = crate::tensor::dot(q, &qkv.row(j)[ko..ko + dk]) * scale;
                }
                softmax_inplace(&mut scores);
                let yr = &mut y.row_mut(i)[qo..qo + dk];
                for j in 0..t_len {
                    let v = &qkv.row(j)[vo..vo + dk];
                    for k in 0..dk {
                        yr[k] += scores[j] * v[k];
                    }
                }
            }
        }
        b.out.forward(&y)
    }
}

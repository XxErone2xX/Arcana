"""
처음부터 만든 미니 트랜스포머 (사전학습 가중치 없음)

참고한 공개 모델 구조:
- BERT: [CLS] 토큰 + 트랜스포머 인코더, [CLS] 출력으로 문장 분류
- JointBERT: 같은 인코더에서 의도(intent) 분류 + 글자별 슬롯 태깅을 동시에 학습
- GPT-2 / 최신 LLM: Pre-LayerNorm (학습이 더 안정적), GELU 활성화

어텐션, 위치 임베딩, 인코더 블록 전부 직접 구현함 (nn.Transformer 안 씀).
"""
import math
import json
import torch
import torch.nn as nn
import torch.nn.functional as F

PAD, UNK, CLS = "<pad>", "<unk>", "<cls>"


# ───────────────────────── 토크나이저 (글자 단위) ─────────────────────────
class CharTokenizer:
    def __init__(self, chars=None):
        self.itos = [PAD, UNK, CLS] + sorted(set(chars or []))
        self.stoi = {c: i for i, c in enumerate(self.itos)}

    def encode(self, text, max_len):
        ids = [self.stoi[CLS]] + [self.stoi.get(c, 1) for c in text.lower()]
        return ids[:max_len]

    def save(self):
        return self.itos

    @classmethod
    def load(cls, itos):
        t = cls()
        t.itos = itos
        t.stoi = {c: i for i, c in enumerate(itos)}
        return t


# ───────────────────────── 멀티헤드 셀프 어텐션 ─────────────────────────
class MultiHeadSelfAttention(nn.Module):
    def __init__(self, d_model, n_heads, dropout):
        super().__init__()
        assert d_model % n_heads == 0
        self.h = n_heads
        self.dk = d_model // n_heads
        self.qkv = nn.Linear(d_model, d_model * 3)
        self.out = nn.Linear(d_model, d_model)
        self.drop = nn.Dropout(dropout)

    def forward(self, x, pad_mask):
        B, T, D = x.shape
        q, k, v = self.qkv(x).chunk(3, dim=-1)
        # (B, 헤드, T, dk) 로 나누기
        q, k, v = [t.view(B, T, self.h, self.dk).transpose(1, 2) for t in (q, k, v)]
        # 어텐션 점수 = QK^T / sqrt(dk)
        scores = q @ k.transpose(-2, -1) / math.sqrt(self.dk)
        # 패딩 위치는 보지 못하게 -무한대
        scores = scores.masked_fill(pad_mask[:, None, None, :], float("-inf"))
        attn = self.drop(F.softmax(scores, dim=-1))
        y = (attn @ v).transpose(1, 2).contiguous().view(B, T, D)
        return self.out(y)


class EncoderBlock(nn.Module):
    """Pre-LN 트랜스포머 블록: x + Attn(LN(x)) → x + FFN(LN(x))"""
    def __init__(self, d_model, n_heads, d_ff, dropout):
        super().__init__()
        self.ln1 = nn.LayerNorm(d_model)
        self.attn = MultiHeadSelfAttention(d_model, n_heads, dropout)
        self.ln2 = nn.LayerNorm(d_model)
        self.ff = nn.Sequential(
            nn.Linear(d_model, d_ff), nn.GELU(), nn.Linear(d_ff, d_model), nn.Dropout(dropout)
        )
        self.drop = nn.Dropout(dropout)

    def forward(self, x, pad_mask):
        x = x + self.drop(self.attn(self.ln1(x), pad_mask))
        x = x + self.ff(self.ln2(x))
        return x


# ───────────────────────── 전체 모델 ─────────────────────────
class AssistantModel(nn.Module):
    def __init__(self, vocab_size, n_intents, n_tags,
                 d_model=128, n_heads=4, n_layers=4, d_ff=512, max_len=64, dropout=0.1):
        super().__init__()
        self.config = dict(vocab_size=vocab_size, n_intents=n_intents, n_tags=n_tags,
                           d_model=d_model, n_heads=n_heads, n_layers=n_layers,
                           d_ff=d_ff, max_len=max_len, dropout=dropout)
        self.tok_emb = nn.Embedding(vocab_size, d_model, padding_idx=0)
        self.pos_emb = nn.Embedding(max_len, d_model)       # BERT식 학습형 위치 임베딩
        self.drop = nn.Dropout(dropout)
        self.blocks = nn.ModuleList([EncoderBlock(d_model, n_heads, d_ff, dropout) for _ in range(n_layers)])
        self.ln_f = nn.LayerNorm(d_model)
        self.intent_head = nn.Linear(d_model, n_intents)   # [CLS] → 의도
        self.slot_head = nn.Linear(d_model, n_tags)        # 각 글자 → 슬롯 태그
        self.apply(self._init)

    @staticmethod
    def _init(m):
        if isinstance(m, (nn.Linear, nn.Embedding)):
            nn.init.normal_(m.weight, std=0.02)
        if isinstance(m, nn.Linear) and m.bias is not None:
            nn.init.zeros_(m.bias)

    def forward(self, ids):
        pad_mask = ids == 0
        pos = torch.arange(ids.size(1), device=ids.device)
        x = self.drop(self.tok_emb(ids) + self.pos_emb(pos)[None])
        for blk in self.blocks:
            x = blk(x, pad_mask)
        x = self.ln_f(x)
        return self.intent_head(x[:, 0]), self.slot_head(x)


# ───────────────────────── 저장/불러오기 ─────────────────────────
def save_bundle(path, model, tok, intents, tags):
    torch.save({
        "config": model.config,
        "state": model.state_dict(),
        "vocab": tok.save(),
        "intents": intents,
        "tags": tags,
    }, path)


def load_bundle(path):
    b = torch.load(path, map_location="cpu", weights_only=False)
    model = AssistantModel(**b["config"])
    model.load_state_dict(b["state"])
    model.eval()
    return model, CharTokenizer.load(b["vocab"]), b["intents"], b["tags"]


@torch.no_grad()
def predict(model, tok, intents, tags, text):
    """문장 → (의도, 확신도, {슬롯: 값})"""
    max_len = model.config["max_len"]
    ids = tok.encode(text, max_len)
    logits_i, logits_s = model(torch.tensor([ids]))
    prob = F.softmax(logits_i[0], -1)
    conf, idx = prob.max(-1)
    slot_ids = logits_s[0].argmax(-1).tolist()[1:]  # [CLS] 제외
    chars = text.lower()[: max_len - 1]
    slots, cur, name = {}, "", None
    for ch, si in zip(chars, slot_ids):
        tag = tags[si]
        if tag.startswith("B-"):
            if name:
                slots.setdefault(name, cur.strip())
            name, cur = tag[2:], ch
        elif tag.startswith("I-") and name == tag[2:]:
            cur += ch
        else:
            if name:
                slots.setdefault(name, cur.strip())
            name, cur = None, ""
    if name:
        slots.setdefault(name, cur.strip())
    return intents[idx.item()], conf.item(), slots

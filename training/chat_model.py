"""
Arcana 대화 모델 (처음부터 구현, 사전학습 가중치 없음)

참고한 공개 구조: GPT-2 / nanoGPT 같은 디코더 트랜스포머 (구조만 참고, 코드·데이터·가중치는 직접)
- 디코더 전용 트랜스포머: 앞 글자들만 보고 다음 글자를 예측 (causal mask)
- Pre-LayerNorm, GELU, 학습형 위치 임베딩
- 입력 형식:  <user>사용자 말<bot>Arcana 대답<eos>
- 학습 때는 대답 부분만 점수 매김 → 사용자 말을 따라 하지 않고 대답하는 법만 배움
"""
import math
import torch
import torch.nn as nn
import torch.nn.functional as F

SPECIAL = ["<pad>", "<unk>", "<user>", "<bot>", "<eos>"]

# 모델 크기 설정. python train_chat.py --size base 처럼 고름
PRESETS = {
    "small": dict(d_model=192, n_heads=6, n_layers=4, d_ff=768, max_len=128),     # 약 200만 — CPU로 15분, exe에 내장
    "base":  dict(d_model=512, n_heads=8, n_layers=8, d_ff=2048, max_len=256),    # 약 2,500만 — 그래픽카드 권장
    "large": dict(d_model=768, n_heads=12, n_layers=14, d_ff=3072, max_len=256),  # 약 1억 — 그래픽카드 + 많은 데이터 필수
}
PAD, UNK, USER, BOT, EOS = range(5)


class CharVocab:
    def __init__(self, chars=()):
        self.itos = SPECIAL + sorted(set(chars) - set(SPECIAL))
        self.stoi = {c: i for i, c in enumerate(self.itos)}

    def enc(self, s, lower=True):
        """사용자 말은 소문자로 통일, Arcana 대답은 대소문자 그대로"""
        return [self.stoi.get(c, UNK) for c in (s.lower() if lower else s)]

    def dec(self, ids):
        return "".join(self.itos[i] for i in ids if i >= len(SPECIAL))

    @classmethod
    def load(cls, itos):
        v = cls()
        v.itos = itos
        v.stoi = {c: i for i, c in enumerate(itos)}
        return v


class CausalSelfAttention(nn.Module):
    def __init__(self, d, h, drop):
        super().__init__()
        self.h, self.dk = h, d // h
        self.qkv = nn.Linear(d, 3 * d)
        self.out = nn.Linear(d, d)
        self.drop = nn.Dropout(drop)

    def forward(self, x):
        B, T, D = x.shape
        q, k, v = self.qkv(x).chunk(3, -1)
        q, k, v = [t.view(B, T, self.h, self.dk).transpose(1, 2) for t in (q, k, v)]
        att = q @ k.transpose(-2, -1) / math.sqrt(self.dk)
        mask = torch.ones(T, T, dtype=torch.bool, device=x.device).triu(1)  # 미래 글자는 못 봄
        att = self.drop(F.softmax(att.masked_fill(mask, float("-inf")), -1))
        return self.out((att @ v).transpose(1, 2).reshape(B, T, D))


class Block(nn.Module):
    def __init__(self, d, h, ff, drop):
        super().__init__()
        self.ln1 = nn.LayerNorm(d)
        self.attn = CausalSelfAttention(d, h, drop)
        self.ln2 = nn.LayerNorm(d)
        self.ff = nn.Sequential(nn.Linear(d, ff), nn.GELU(), nn.Linear(ff, d), nn.Dropout(drop))
        self.drop = nn.Dropout(drop)

    def forward(self, x):
        x = x + self.drop(self.attn(self.ln1(x)))
        return x + self.ff(self.ln2(x))


class ChatGPT(nn.Module):
    def __init__(self, vocab_size, d_model=192, n_heads=6, n_layers=4, d_ff=768, max_len=128, dropout=0.1):
        super().__init__()
        self.config = dict(vocab_size=vocab_size, d_model=d_model, n_heads=n_heads,
                           n_layers=n_layers, d_ff=d_ff, max_len=max_len)
        self.tok_emb = nn.Embedding(vocab_size, d_model)
        self.pos_emb = nn.Embedding(max_len, d_model)
        self.drop = nn.Dropout(dropout)
        self.blocks = nn.ModuleList([Block(d_model, n_heads, d_ff, dropout) for _ in range(n_layers)])
        self.ln_f = nn.LayerNorm(d_model)
        self.lm_head = nn.Linear(d_model, vocab_size)
        self.apply(self._init)

    @staticmethod
    def _init(m):
        if isinstance(m, (nn.Linear, nn.Embedding)):
            nn.init.normal_(m.weight, std=0.02)
        if isinstance(m, nn.Linear) and m.bias is not None:
            nn.init.zeros_(m.bias)

    def forward(self, ids):
        pos = torch.arange(ids.size(1), device=ids.device)
        x = self.drop(self.tok_emb(ids) + self.pos_emb(pos)[None])
        for b in self.blocks:
            x = b(x)
        return self.lm_head(self.ln_f(x))

    @torch.no_grad()
    def reply(self, vocab, text, temperature=0.0, top_k=8, max_new=80):
        """temperature 0 = 항상 가장 확률 높은 글자 (테스트용)"""
        ids = [USER] + vocab.enc(text)[: self.config["max_len"] // 2] + [BOT]
        out = []
        for _ in range(max_new):
            if len(ids) >= self.config["max_len"]:
                break
            logits = self(torch.tensor([ids]))[0, -1]
            if temperature <= 0:
                nxt = int(logits.argmax())
            else:
                logits = logits / temperature
                v, i = logits.topk(top_k)
                nxt = int(i[torch.multinomial(F.softmax(v, -1), 1)])
            if nxt == EOS:
                break
            ids.append(nxt)
            out.append(nxt)
        return vocab.dec(out)

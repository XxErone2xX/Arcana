"""
공책으로 학습하기: 학습 탭에서 공부한 문서로 대화 모델을 처음부터 다시 학습

  python pretrain_notes.py                 # 그래픽카드 있으면 base(약 2,600만), 없으면 small(약 200만)
  python pretrain_notes.py --size small    # 크기 직접 지정

  1단계 사전학습: 공책 문서(%APPDATA%\\Arcana\\knowledge\\docs.jsonl)를 읽으며 "다음 글자 맞히기"
                 → 한국어 문장 자체(어휘, 문법, 문장 흐름)를 배움
  2단계 대화 학습: data/*.jsonl 대화 + 가르쳐 준 대답(taught.jsonl)으로 말하는 법을 배움
                 (1단계 실력을 잊지 않게 사전학습 글도 조금씩 섞음)
  결과: %APPDATA%\\Arcana\\models\\arcana-chat.*  (이전 모델은 models\\backup-날짜\\ 에 보관)
        → 학습 탭에서 /Reload 하거나 앱을 다시 켜면 적용

쓰는 문서: 위키백과에서 온 것 + 직접 가르쳐 준 것만 (위키백과는 CC BY-SA 라 학습에 써도 됨.
           일반 웹사이트 글은 저작권이 있을 수 있어서 안 씀)
"""
import argparse
import collections
import datetime
import json
import os
import random
import re
import shutil
import time
import torch
import torch.nn.functional as F
from chat_model import ChatGPT, CharVocab, PRESETS, USER, BOT, EOS, PAD
from export import write_safetensors
from train_feedback import default_dir, read_jsonl
from train_chat import known_inputs

HERE = os.path.dirname(os.path.abspath(__file__))
TEST = ["뭐 해?", "나 내일 발표야.", "배고파", "어제 영화 보고 왔어.", "주말에 뭐 했어?", "오늘 너무 춥다.",
        "나 시험 망했어", "심심해", "같이 밥 먹을래?", "나 이어폰 잃어버렸어", "방학 때 뭐 할 거야?", "피곤하다 진짜"]
DEFAULTS = {
    "small": dict(pre_steps=1500, sft_steps=1500, batch=64, lr=2e-3),
    "base": dict(pre_steps=6000, sft_steps=3000, batch=64, lr=6e-4),
    "large": dict(pre_steps=20000, sft_steps=6000, batch=48, lr=3e-4),
}


def korean_ratio(s):
    letters = [c for c in s if c.isalpha()]
    return sum("가" <= c <= "힣" for c in letters) / max(len(letters), 1)


def load_corpus(kdir):
    docs = read_jsonl(os.path.join(kdir, "docs.jsonl"))
    texts, skipped = [], collections.Counter()
    for d in docs:
        url, src, text = d.get("url", ""), d.get("source", ""), d.get("summary", "").strip()
        if not ("wikipedia.org" in url or src == "사용자"):
            skipped["위키백과 아님(저작권)"] += 1
            continue
        if korean_ratio(text) < 0.5:
            skipped["한국어 아님"] += 1
            continue
        if len(text) < 20:
            skipped["너무 짧음"] += 1
            continue
        if src == "사용자" and not text.startswith(d.get("title", "")):
            text = f"{d['title']}: {text}"  # 직접 가르쳐 준 뜻은 "단어: 뜻" 형태로
        texts.append(text)
    return texts, len(docs), skipped


def load_dialogue(kdir):
    pairs = []
    ddir = os.path.join(HERE, "data")
    if os.path.isdir(ddir):
        for f in sorted(os.listdir(ddir)):
            if f.endswith(".jsonl"):
                pairs += [(r["user"], r["bot"]) for r in read_jsonl(os.path.join(ddir, f)) if r.get("user") and r.get("bot")]
    taught = [(t["user"], t["bot"]) for t in read_jsonl(os.path.join(kdir, "taught.jsonl"))]
    better = [(f["user"], f["better"]) for f in read_jsonl(os.path.join(kdir, "feedback.jsonl")) if f.get("better")]
    return pairs, taught + better


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=default_dir())
    ap.add_argument("--size", choices=PRESETS)
    ap.add_argument("--pre-steps", type=int)
    ap.add_argument("--sft-steps", type=int)
    ap.add_argument("--min-char", type=int, default=3, help="이보다 적게 나온 글자는 사전에서 뺌 (한자·특수문자 정리)")
    args = ap.parse_args()
    random.seed(0)
    torch.manual_seed(0)
    torch.set_num_threads(os.cpu_count() or 2)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    amp = device == "cuda"
    size = args.size or ("base" if device == "cuda" else "small")
    d = DEFAULTS[size]
    pre_steps, sft_steps = args.pre_steps or d["pre_steps"], args.sft_steps or d["sft_steps"]
    kdir = os.path.join(args.dir, "knowledge")

    corpus, n_docs, skipped = load_corpus(kdir)
    pairs, mine = load_dialogue(kdir)
    print(f"공책 문서 {n_docs:,}개 중 {len(corpus):,}개 사용 ({sum(map(len, corpus)):,}글자)  제외: {dict(skipped)}")
    print(f"대화 데이터 {len(pairs):,}쌍 + 직접 가르친 대답 {len(mine)}개")
    if len(corpus) < 50:
        print("공책 문서가 너무 적습니다. 학습 탭에서 /Start 로 위키백과를 더 공부한 뒤 다시 해 주세요.")
        return
    if not pairs and len(mine) < 30:
        print("대화 데이터가 없습니다. 말하는 법(2단계)을 배울 대화가 필요합니다.")
        print("  - 비서다운 공손한 대화 데이터를 training/data/ 에 넣어 주세요 (convert_dialogue.py 로 변환 가능)")
        print("    추천: AI Hub '민원(콜센터) 질의-응답', '용도별 목적대화' (공손한 존댓말 상담 대화)")
        print("  - 또는 학습 탭 /Teach, 대화 탭 👎+더 좋은 대답으로 대답을 30개 이상 가르쳐 주세요")
        return
    dialog = pairs + mine * 5  # 직접 가르친 건 더 중요하게

    # 글자 사전: 자주 나온 글자 + 대화에 나온 글자는 전부
    count = collections.Counter(c for t in corpus for c in t)
    chars = {c for c, n in count.items() if n >= args.min_char}
    for u, r in dialog:
        chars.update(u.lower() + r)
    vocab = CharVocab(chars)
    model = ChatGPT(len(vocab.itos), **PRESETS[size]).to(device)
    L = model.config["max_len"]
    n_params = sum(p.numel() for p in model.parameters())
    print(f"모델 {size} · 파라미터 {n_params:,} · 글자 {len(vocab.itos)}종 · {device}" + (" (bf16)" if amp else ""))

    # 1단계용: 문서들을 이어 붙인 긴 글자 줄
    stream = []
    for t in corpus:
        stream += vocab.enc(t, lower=False) + [EOS]
    stream = torch.tensor(stream, dtype=torch.long)

    def lm_batch(n):
        idx = torch.randint(0, len(stream) - L - 1, (n,))
        x = torch.stack([stream[i:i + L] for i in idx])
        y = torch.stack([stream[i + 1:i + L + 1] for i in idx])
        return x.to(device), y.to(device)

    def sft_batch(items):
        xs, ys = [], []
        for u, r in items:
            p = [USER] + vocab.enc(u)[: L // 2] + [BOT]
            seq = (p + vocab.enc(r, lower=False) + [EOS])[: L + 1]
            x, y = seq[:-1], seq[1:]
            xs.append(x)
            ys.append([-100] * (len(p) - 1) + y[len(p) - 1:])
        T = max(map(len, xs))
        xs = [x + [PAD] * (T - len(x)) for x in xs]
        ys = [y + [-100] * (T - len(y)) for y in ys]
        return torch.tensor(xs, device=device), torch.tensor(ys, device=device)

    def loss_of(x, y):
        with torch.autocast(device_type="cuda", dtype=torch.bfloat16, enabled=amp):
            logits = model(x)
        return F.cross_entropy(logits.float().reshape(-1, len(vocab.itos)), y.reshape(-1), ignore_index=-100)

    def run(stage, steps, lr, make):
        opt = torch.optim.AdamW(model.parameters(), lr=lr, weight_decay=0.05, betas=(0.9, 0.95))
        sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=lr, total_steps=steps, pct_start=0.05)
        model.train()
        t0, avg = time.time(), None
        for step in range(1, steps + 1):
            loss = make()
            opt.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            sched.step()
            avg = loss.item() if avg is None else avg * 0.98 + loss.item() * 0.02
            if step % 250 == 0 or step == steps:
                el = time.time() - t0
                print(f"[{stage}] {step}/{steps}  loss {avg:.3f}  ({el:.0f}초, 남은 시간 약 {el / step * (steps - step) / 60:.0f}분)", flush=True)

    @torch.no_grad()
    def show(title):
        model.eval()
        print(f"\n── {title} ──")
        for t in TEST[:6]:
            print(f"{t}  →  {model.reply(vocab, t, temperature=0.7)}")
        model.train()

    print("\n1단계: 공책 문서로 한국어 배우기")
    run("사전학습", pre_steps, d["lr"], lambda: loss_of(*lm_batch(d["batch"])))
    model.eval()
    with torch.no_grad():  # 사전학습이 된 걸 확인: 문서 첫머리를 주고 이어 쓰게 함
        seed = corpus[0][:12]
        ids = vocab.enc(seed, lower=False)
        for _ in range(60):
            nxt = int(model(torch.tensor([ids[-L:]], device=device))[0, -1].argmax())
            if nxt == EOS:
                break
            ids.append(nxt)
        print(f"이어 쓰기 시험: {vocab.dec(ids)}")

    print("\n2단계: 대화로 말하는 법 배우기")
    run("대화학습", sft_steps, d["lr"] * 0.5,
        lambda: loss_of(*sft_batch(random.choices(dialog, k=d["batch"]))) + 0.2 * loss_of(*lm_batch(d["batch"] // 4)))
    show("학습 결과")

    # 저장 (이전 모델 백업)
    model.eval().cpu().float()
    out = os.path.join(args.dir, "models")
    os.makedirs(out, exist_ok=True)
    if os.path.exists(os.path.join(out, "arcana-chat.safetensors")):
        bk = os.path.join(out, "backup-" + datetime.datetime.now().strftime("%Y%m%d-%H%M%S"))
        os.makedirs(bk)
        for f in ("arcana-chat.safetensors", "arcana-chat.json"):
            if os.path.exists(os.path.join(out, f)):
                shutil.copy2(os.path.join(out, f), bk)
    write_safetensors(os.path.join(out, "arcana-chat.safetensors"), model.state_dict())
    meta = {
        "format_version": 1,
        "architecture": "arcana-chat-v1",
        "config": model.config,
        "layer_norm_eps": 1e-5,
        "vocab": vocab.itos,
        "special": {"pad": 0, "unk": 1, "user": 2, "bot": 3, "eos": 4},
        "known_inputs": known_inputs([u for u, _ in dialog]),
        "pretrained_on": f"공책 문서 {len(corpus)}개 ({sum(map(len, corpus))}글자)",
        "trained_at": datetime.datetime.now().strftime("%Y-%m-%d %H:%M"),
    }
    with open(os.path.join(out, "arcana-chat.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=1)
    print(f"\n완료! → {out}\n학습 탭에서 /Reload 를 입력하거나 앱을 다시 켜면 새 모델로 대화합니다.")


if __name__ == "__main__":
    main()

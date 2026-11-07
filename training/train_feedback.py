"""
3단계: 피드백 학습 (DPO — Direct Preference Optimization)

앱에서 모인 👍/👎, 더 좋은 대답, 가르쳐 준 대답으로 대화 모델을 다시 학습시킨다.

  python train_feedback.py                 # %APPDATA%\\Arcana 의 피드백으로 학습 → 앱에 바로 적용
  python train_feedback.py --steps 600     # 더 오래

큰 AI들의 3단계(RLHF)는 "좋은 대답 판별기"를 따로 만든 다음 강화학습을 하는데,
DPO는 판별기 없이 바로 "좋은 대답 확률은 올리고, 나쁜 대답 확률은 내리는" 방식이라 훨씬 가볍다.
(학습 전 모델을 기준으로 삼아서, 너무 멀리 벗어나 말투가 망가지지 않게 붙잡아 둠)

쓰는 데이터 (%APPDATA%\\Arcana\\knowledge\\):
  feedback.jsonl  👎 + 더 좋은 대답 → (좋은 대답, 나쁜 대답) 쌍
                  👍 → 좋은 대답 / 같은 말에 👎 받은 대답이 있으면 쌍
                  👎만 → 그 대답이 덜 나오게
  taught.jsonl    가르쳐 준 대답 → 좋은 대답
결과: %APPDATA%\\Arcana\\models\\arcana-chat.*  (이전 모델은 models\\backup-날짜\\ 에 보관)
"""
import argparse
import copy
import datetime
import json
import os
import random
import re
import shutil
import torch
import torch.nn.functional as F
from chat_data import generate
from chat_model import ChatGPT, CharVocab, USER, BOT, EOS, PAD
from export import read_safetensors, write_safetensors

HERE = os.path.dirname(os.path.abspath(__file__))
BUILTIN = os.path.join(HERE, "..", "models")


def default_dir():
    if os.environ.get("APPDATA"):
        return os.path.join(os.environ["APPDATA"], "Arcana")
    return os.path.join(os.path.expanduser("~"), ".arcana")


def key(s):
    s = re.sub(r"[^\w]", "", s.lower())
    core = re.sub(r"[ㄱ-ㆎ]", "", s)
    return core or s


def read_jsonl(path):
    if not os.path.exists(path):
        return []
    with open(path, encoding="utf-8") as f:
        return [json.loads(l) for l in f if l.strip()]


def load_model(models_dir):
    meta = json.load(open(os.path.join(models_dir, "arcana-chat.json"), encoding="utf-8"))
    cfg = {k: v for k, v in meta["config"].items() if k != "vocab_size"}
    model = ChatGPT(len(meta["vocab"]), **cfg)
    model.load_state_dict(read_safetensors(os.path.join(models_dir, "arcana-chat.safetensors")))
    return model, CharVocab.load(meta["vocab"]), meta


def extend_vocab(model, vocab, texts):
    """피드백에 처음 보는 글자가 있으면 사전과 임베딩을 늘림"""
    new = sorted({c for t in texts for c in t} - set(vocab.itos))
    if not new:
        return 0
    old_n, d = model.tok_emb.weight.shape
    vocab.itos += new
    vocab.stoi = {c: i for i, c in enumerate(vocab.itos)}
    n = len(vocab.itos)
    emb = torch.nn.Embedding(n, d)
    head = torch.nn.Linear(d, n)
    with torch.no_grad():
        torch.nn.init.normal_(emb.weight, std=0.02)
        emb.weight[:old_n] = model.tok_emb.weight
        torch.nn.init.normal_(head.weight, std=0.02)
        head.bias.fill_(float(model.lm_head.bias.mean()))  # 새 글자는 보통 글자 수준에서 시작
        head.weight[:old_n] = model.lm_head.weight
        head.bias[:old_n] = model.lm_head.bias
    model.tok_emb, model.lm_head = emb, head
    model.config["vocab_size"] = n
    return len(new)


def build(feedback, taught):
    pairs, good, bad = [], [], []
    by_prompt = {}
    for f in feedback:
        by_prompt.setdefault(key(f["user"]), []).append(f)
    for f in feedback:
        u, r = f["user"], f["reply"]
        if f["rating"] < 0 and f.get("better"):
            pairs.append((u, f["better"], r))
            good.append((u, f["better"]))
        elif f["rating"] > 0:
            good.append((u, r))
            for g in by_prompt[key(u)]:
                if g["rating"] < 0 and key(g["reply"]) != key(r):
                    pairs.append((u, r, g["reply"]))
        else:
            if not any(g["rating"] > 0 or g.get("better") for g in by_prompt[key(u)]):
                bad.append((u, r))
    good += [(t["user"], t["bot"]) for t in taught]
    return pairs, good, bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=default_dir(), help="Arcana 데이터 폴더 (기본: %%APPDATA%%\\Arcana)")
    ap.add_argument("--steps", type=int, default=300)
    ap.add_argument("--lr", type=float, default=1e-4)
    ap.add_argument("--beta", type=float, default=0.1, help="DPO 강도 (작을수록 원래 모델에서 멀리 감)")
    args = ap.parse_args()
    torch.manual_seed(0)
    random.seed(0)
    device = "cuda" if torch.cuda.is_available() else "cpu"

    kdir = os.path.join(args.dir, "knowledge")
    feedback = read_jsonl(os.path.join(kdir, "feedback.jsonl"))
    taught = read_jsonl(os.path.join(kdir, "taught.jsonl"))
    pairs, good, bad = build(feedback, taught)
    print(f"피드백 {len(feedback)}개 · 가르친 대답 {len(taught)}개 → 비교 쌍 {len(pairs)} · 좋은 대답 {len(good)} · 나쁜 대답 {len(bad)}")
    if not (pairs or good or bad):
        print("학습할 피드백이 없습니다. 앱에서 👍/👎를 눌러 주세요.")
        return

    user_models = os.path.join(args.dir, "models")
    src = user_models if os.path.exists(os.path.join(user_models, "arcana-chat.safetensors")) else BUILTIN
    model, vocab, meta = load_model(src)
    added = extend_vocab(model, vocab, [t for p in pairs for t in p] + [t for g in good for t in g] + [t for b in bad for t in b])
    print(f"시작 모델: {os.path.abspath(src)}  (새 글자 {added}개 추가)")
    model.to(device)
    ref = copy.deepcopy(model).eval()  # 기준 모델 (고정)
    for p in ref.parameters():
        p.requires_grad_(False)
    L = model.config["max_len"]

    def encode(u, r):
        p = [USER] + vocab.enc(u)[: L // 2] + [BOT]
        seq = (p + vocab.enc(r, lower=False) + [EOS])[: L + 1]
        x, y = seq[:-1], seq[1:]
        return x, [-100] * (len(p) - 1) + y[len(p) - 1:]

    def batch(items):
        enc = [encode(u, r) for u, r in items]
        T = max(len(x) for x, _ in enc)
        xs = torch.tensor([x + [PAD] * (T - len(x)) for x, _ in enc], device=device)
        ys = torch.tensor([y + [-100] * (T - len(y)) for _, y in enc], device=device)
        return xs, ys

    def seq_logp(m, items):
        """대답 전체의 로그 확률 (문장마다 하나)"""
        x, y = batch(items)
        logp = F.log_softmax(m(x).float(), -1)
        mask = y != -100
        tok = logp.gather(-1, y.clamp(min=0).unsqueeze(-1)).squeeze(-1)
        return (tok * mask).sum(-1), x, y

    @torch.no_grad()
    def report(title):
        if pairs:
            m = model.eval()
            c = seq_logp(m, [(u, ch) for u, ch, _ in pairs])[0]
            r = seq_logp(m, [(u, rj) for u, _, rj in pairs])[0]
            print(f"{title}: 좋은 대답을 더 선호하는 비율 {(c > r).float().mean():.0%}  (평균 차이 {(c - r).mean():+.1f})")
        model.train()

    anchor = generate(4000, seed=123)  # 원래 대화 실력 유지용
    report("학습 전")
    opt = torch.optim.AdamW([p for p in model.parameters() if p.requires_grad], lr=args.lr, weight_decay=0.0)
    model.train()
    for step in range(1, args.steps + 1):
        loss = torch.zeros((), device=device)
        if pairs:  # DPO
            b = random.choices(pairs, k=min(16, max(4, len(pairs))))
            pc = seq_logp(model, [(u, c) for u, c, _ in b])[0]
            pr = seq_logp(model, [(u, r) for u, _, r in b])[0]
            with torch.no_grad():
                rc = seq_logp(ref, [(u, c) for u, c, _ in b])[0]
                rr = seq_logp(ref, [(u, r) for u, _, r in b])[0]
            loss = loss - F.logsigmoid(args.beta * ((pc - rc) - (pr - rr))).mean()
        if good:  # 좋은 대답 따라 하기
            x, y = batch(random.choices(good, k=16))
            loss = loss + 0.5 * F.cross_entropy(model(x).float().reshape(-1, len(vocab.itos)), y.reshape(-1), ignore_index=-100)
        if bad:  # 나쁜 대답은 덜 나오게 (unlikelihood)
            x, y = batch(random.choices(bad, k=8))
            p = F.softmax(model(x).float(), -1).gather(-1, y.clamp(min=0).unsqueeze(-1)).squeeze(-1)
            m = (y != -100).float()
            loss = loss + 0.3 * (-(torch.log((1 - p).clamp(min=1e-5)) * m).sum() / m.sum())
        x, y = batch(random.sample(anchor, 16))  # 원래 실력 유지
        loss = loss + 0.5 * F.cross_entropy(model(x).float().reshape(-1, len(vocab.itos)), y.reshape(-1), ignore_index=-100)
        opt.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        opt.step()
        if step % 50 == 0:
            print(f"step {step}/{args.steps}  loss {loss.item():.3f}", flush=True)
    report("학습 후")

    model.eval().cpu()
    print("\n── 피드백 받은 말에 지금 대답하면 ──")
    seen = set()
    for u, *_ in pairs + good + bad:
        if key(u) in seen or len(seen) >= 10:
            continue
        seen.add(key(u))
        print(f"{u}  →  {model.reply(vocab, u, temperature=0.0)}")

    # 저장 (이전 모델은 백업)
    os.makedirs(user_models, exist_ok=True)
    if src == user_models:
        bk = os.path.join(user_models, "backup-" + datetime.datetime.now().strftime("%Y%m%d-%H%M%S"))
        os.makedirs(bk)
        for f in ("arcana-chat.safetensors", "arcana-chat.json"):
            shutil.copy2(os.path.join(user_models, f), bk)
    # 오래된 백업은 최근 3개만 남김
    bks = sorted(d for d in os.listdir(user_models) if d.startswith("backup-") and os.path.isdir(os.path.join(user_models, d)))
    for d in bks[:-3]:
        shutil.rmtree(os.path.join(user_models, d), ignore_errors=True)
    write_safetensors(os.path.join(user_models, "arcana-chat.safetensors"), model.state_dict())
    meta["config"] = model.config
    meta["vocab"] = vocab.itos
    known = set(meta.get("known_inputs", []))
    known.update(u for u, *_ in pairs + good)  # 가르쳐 준 말은 이제 "아는 말"
    meta["known_inputs"] = sorted(known)
    meta["feedback_trained"] = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
    with open(os.path.join(user_models, "arcana-chat.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=1)
    print(f"\n완료! → {os.path.abspath(user_models)}\n앱을 다시 켜면 새 모델로 대화합니다.")


if __name__ == "__main__":
    main()

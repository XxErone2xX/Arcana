"""
공책으로 학습하기: 학습 탭에서 공부한 문서로 대화 모델을 처음부터 다시 학습

  python pretrain_notes.py                 # 그래픽카드 있으면 base(약 2,600만), 없으면 small(약 200만)
  python pretrain_notes.py --size small    # 크기 직접 지정

  1단계 사전학습: 공책 문서 본문(%APPDATA%\\Arcana\\knowledge\\fulltext.jsonl, 없으면 docs.jsonl 요약)을 읽으며 "다음 글자 맞히기"
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
import hashlib
import shutil
import sys
import time
import torch
import torch.nn.functional as F
from chat_model import ChatGPT, CharVocab, PRESETS, USER, BOT, EOS, PAD
from export import read_safetensors, write_safetensors
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


# ───────────── 재료 다듬기 ─────────────
URL_RE = re.compile(r"https?://\S+|www\.\S+")
REPEAT_RE = re.compile(r"(.)\1{4,}")            # ㅋㅋㅋㅋㅋㅋ → ㅋㅋㅋ
SPACE_RE = re.compile(r"[ \t ​]+")
# 위키백과 문서 끝부분 (여기부터는 목록·출처라 문장 공부에 안 좋음)
TAIL_SECTIONS = {"같이 보기", "각주", "외부 링크", "참고 문헌", "참고 자료", "출처", "주석", "관련 항목", "더 읽어보기"}
JUNK_WORDS = ("무단 전재", "무단전재", "copyright", "©", "all rights reserved", "로그인", "회원가입", "쿠키", "개인정보처리방침",
              "이용약관", "구독하기", "공유하기", "좋아요", "댓글", "광고", "바로가기", "본문 바로가기", "기사제보")


def clean_lines(text):
    out = []
    for line in text.split("\n"):
        line = SPACE_RE.sub(" ", URL_RE.sub("", line)).strip()
        line = REPEAT_RE.sub(lambda m: m.group(1) * 3, line)
        if line.strip("= ") in TAIL_SECTIONS:
            break
        out.append(line)
    return out


def norm_key(s):
    return re.sub(r"[^0-9a-z가-힣]", "", s.lower())


def tidy_corpus(raw_texts):
    """중복 문서 제거 · 여러 문서에 반복되는 줄(메뉴·저작권 문구) 제거 · 짧거나 깨진 줄 제거"""
    stats = collections.Counter()
    docs = [clean_lines(t) for t in raw_texts]
    # 여러 문서(5개 이상)에 똑같이 나오는 줄 = 사이트 공통 문구
    line_docs = collections.Counter()
    for lines in docs:
        for k in {norm_key(l) for l in lines if l}:
            line_docs[k] += 1
    boiler = {k for k, n in line_docs.items() if n >= 5 and len(k) < 200}
    out, seen = [], set()
    for lines in docs:
        keep = []
        for l in lines:
            k = norm_key(l)
            if not k:
                continue
            if k in boiler:
                stats["반복 문구 줄"] += 1
                continue
            low = l.lower()
            if len(l) < 6 or any(w in low for w in JUNK_WORDS) and len(l) < 80:
                stats["짧거나 쓸모없는 줄"] += 1
                continue
            if korean_ratio(l) < 0.3 and not re.search(r"\d", l):
                stats["한국어 아닌 줄"] += 1
                continue
            keep.append(l)
        text = "\n".join(keep).strip()
        if len(text) < 100:
            stats["다듬고 나니 너무 짧음"] += 1
            continue
        k = norm_key(text)
        key = (k[:300], k[-300:], len(k) // 200)  # 앞·뒤·길이가 같으면 같은 문서로 봄
        if key in seen:
            stats["중복 문서"] += 1
            continue
        seen.add(key)
        out.append(text)
    return out, stats


def load_corpus(kdir, max_chars):
    """1단계 재료: fulltext.jsonl(본문 전체) + 본문이 없는 문서는 docs.jsonl 요약 → 다듬기"""
    full = read_jsonl(os.path.join(kdir, "fulltext.jsonl"))
    docs = read_jsonl(os.path.join(kdir, "docs.jsonl"))
    texts, skipped, have = [], collections.Counter(), set()
    for d in full:
        text = d.get("text", "").strip()
        if korean_ratio(text) < 0.5:
            skipped["한국어 아님"] += 1
            continue
        texts.append(text)
        have.add(d.get("url", "") or d.get("title", ""))
    for d in docs:
        src, text = d.get("source", ""), d.get("summary", "").strip()
        if (d.get("url", "") or d.get("title", "")) in have:
            continue
        if korean_ratio(text) < 0.5:
            skipped["한국어 아님"] += 1
            continue
        if src == "사용자" and not text.startswith(d.get("title", "")):
            text = f"{d['title']}: {text}"  # 직접 가르쳐 준 뜻은 "단어: 뜻" 형태로
        texts.append(text)
    # 직접 넣은 글 (corpus 폴더의 .jsonl, 한 줄에 {"text": ...}) — 예: AI Hub 문서 변환본
    cdir = os.path.join(HERE, "corpus")
    if os.path.isdir(cdir):
        for f in sorted(os.listdir(cdir)):
            if f.endswith(".jsonl"):
                extra = [r.get("text", "").strip() for r in read_jsonl(os.path.join(cdir, f))]
                extra = [t for t in extra if korean_ratio(t) >= 0.5]
                print(f"  corpus/{f}: {len(extra):,}개")
                texts = extra + texts  # 앞에 둠 → 용량이 넘치면 공책 문서(최근)가 우선
    texts, tidy = tidy_corpus(texts)
    skipped.update(tidy)
    # 너무 많으면 최근 것 위주로 (메모리 보호)
    total, keep = 0, []
    for t in reversed(texts):
        if total + len(t) > max_chars:
            skipped["용량 초과(오래된 것)"] += 1
            continue
        keep.append(t)
        total += len(t)
    keep.reverse()
    return keep, len(docs), skipped, len(full)


def text_hash(t):
    return hashlib.sha1(norm_key(t).encode("utf-8")).hexdigest()[:16]


def load_previous(out, size):
    """예전 모델이 같은 크기면 (글자 목록, 가중치, 배운 문서 기록)"""
    meta_p, w_p = os.path.join(out, "arcana-chat.json"), os.path.join(out, "arcana-chat.safetensors")
    if not (os.path.exists(meta_p) and os.path.exists(w_p)):
        return None
    try:
        meta = json.load(open(meta_p, encoding="utf-8"))
        cfg = dict(meta["config"])
        cfg.pop("vocab_size", None)
        want = {k: v for k, v in ChatGPT(10, **PRESETS[size]).config.items() if k != "vocab_size"}
        if any(cfg.get(k) != v for k, v in want.items() if k != "dropout"):
            return None
        seen_p = os.path.join(out, "arcana-chat.seen")
        seen = set(open(seen_p, encoding="utf-8").read().split()) if os.path.exists(seen_p) else set()
        return meta["vocab"], read_safetensors(w_p), seen
    except Exception as e:
        print(f"예전 모델을 읽지 못해서 처음부터 학습합니다. ({e})")
        return None


def grow_into(model, old_state, n_old):
    """예전 가중치를 새 모델에 넣기. 글자 수가 늘었으면 새 글자 줄만 새로 시작"""
    new_state = model.state_dict()
    for k, v in old_state.items():
        if k not in new_state:
            continue
        tgt = new_state[k]
        if tgt.shape == v.shape:
            new_state[k] = v
        elif k in ("tok_emb.weight", "lm_head.weight", "lm_head.bias") and v.shape[0] == n_old:
            tgt[:n_old] = v
            new_state[k] = tgt
    model.load_state_dict(new_state)


def clean_backups(out, keep=3):
    """오래된 백업 지우기 (최근 keep 개만 남김)"""
    bks = sorted(d for d in os.listdir(out) if d.startswith("backup-") and os.path.isdir(os.path.join(out, d)))
    for d in bks[:-keep]:
        shutil.rmtree(os.path.join(out, d), ignore_errors=True)


def load_dialogue(kdir):
    pairs = []
    ddir = os.path.join(HERE, "data")
    if os.path.isdir(ddir):
        for f in sorted(os.listdir(ddir)):
            if f.endswith(".jsonl"):
                pairs += [(r["user"], r["bot"]) for r in read_jsonl(os.path.join(ddir, f)) if r.get("user") and r.get("bot")]
    taught = [(t["user"], t["bot"]) for t in read_jsonl(os.path.join(kdir, "taught.jsonl"))]
    better = [(f["user"], f["better"]) for f in read_jsonl(os.path.join(kdir, "feedback.jsonl")) if f.get("better")]

    def dedup(items):  # 같은 쌍은 한 번만, 앞뒤 공백 정리, 너무 긴 건 제외
        seen, out = set(), []
        for u, r in items:
            u, r = u.strip(), r.strip()
            k = (norm_key(u), norm_key(r))
            if not k[0] or not k[1] or k in seen or len(r) > 400:
                continue
            seen.add(k)
            out.append((u, r))
        return out
    return dedup(pairs), dedup(taught + better)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=default_dir())
    ap.add_argument("--size", choices=PRESETS)
    ap.add_argument("--pre-steps", type=int)
    ap.add_argument("--sft-steps", type=int)
    ap.add_argument("--fresh", action="store_true", help="이어서 학습하지 않고 처음부터 새로 학습")
    ap.add_argument("--max-chars", type=int, default=200_000_000, help="1단계에 쓸 최대 글자 수 (메모리 보호)")
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

    corpus, n_docs, skipped, n_full = load_corpus(kdir, args.max_chars)
    pairs, mine = load_dialogue(kdir)
    print(f"공책 문서 {n_docs:,}개 (본문 전체 {n_full:,}개) 중 {len(corpus):,}개 사용 ({sum(map(len, corpus)):,}글자)  제외: {dict(skipped)}")
    print(f"대화 데이터 {len(pairs):,}쌍 + 직접 가르친 대답 {len(mine)}개")
    if len(corpus) < 50:
        print("공책 문서가 너무 적습니다. 학습 탭에서 /Start 로 위키백과를 더 공부한 뒤 다시 해 주세요.")
        sys.exit(2)
    if not pairs and len(mine) < 30:
        print("대화 데이터가 없습니다. 말하는 법(2단계)을 배울 대화가 필요합니다.")
        print("  - 비서다운 공손한 대화 데이터를 training/data/ 에 넣어 주세요 (convert_dialogue.py 로 변환 가능)")
        print("    추천: AI Hub '민원(콜센터) 질의-응답', '용도별 목적대화' (공손한 존댓말 상담 대화)")
        print("  - 또는 학습 탭 /Teach, 대화 탭 👎+더 좋은 대답으로 대답을 30개 이상 가르쳐 주세요")
        sys.exit(2)
    dialog = pairs + mine * 5  # 직접 가르친 건 더 중요하게

    # 글자 사전: 자주 나온 글자 + 대화에 나온 글자는 전부
    count = collections.Counter(c for t in corpus for c in t)
    chars = {c for c, n in count.items() if n >= args.min_char}
    for u, r in dialog:
        chars.update(u.lower() + r)
    out = os.path.join(args.dir, "models")
    prev = None if args.fresh else load_previous(out, size)
    if prev:
        # 이어서 학습: 예전 글자 번호는 그대로 두고 새 글자만 뒤에 붙임
        old_itos, old_state, old_seen = prev
        vocab = CharVocab.load(old_itos + sorted(chars - set(old_itos)))
        model = ChatGPT(len(vocab.itos), **PRESETS[size])
        grow_into(model, old_state, len(old_itos))
        model.to(device)
        seen = old_seen
        new_texts = [t for t in corpus if text_hash(t) not in seen]
        # 새로 배울 게 적으면 조금만, 많으면 많이 (처음부터보다는 훨씬 짧게)
        frac = min(1.0, len(new_texts) / max(len(corpus), 1))
        pre_steps = max(200, int(pre_steps * (0.15 + 0.35 * frac)))
        sft_steps = max(200, int(sft_steps * 0.4))
        lr_scale = 0.4
        print(f"이어서 학습: 예전 모델에서 시작 · 새 문서 {len(new_texts):,}개 · 새 글자 {len(vocab.itos) - len(old_itos)}종")
    else:
        vocab = CharVocab(chars)
        model = ChatGPT(len(vocab.itos), **PRESETS[size]).to(device)
        seen, new_texts, lr_scale = set(), [], 1.0
        print("처음부터 학습합니다." if args.fresh or not os.path.exists(os.path.join(out, "arcana-chat.json")) else "예전 모델과 크기가 달라서 처음부터 학습합니다.")
    L = model.config["max_len"]
    n_params = sum(p.numel() for p in model.parameters())
    print(f"모델 {size} · 파라미터 {n_params:,} · 글자 {len(vocab.itos)}종 · {device}" + (" (bf16)" if amp else ""))

    # 1단계용: 문서들을 이어 붙인 긴 글자 줄
    def to_stream(texts):
        # 글자 번호를 4바이트씩 촘촘히 (파이썬 리스트보다 메모리를 약 10배 덜 씀 → 위키백과 덤프도 들어감)
        import array
        ids = array.array("i")
        for t in texts:
            ids.extend(vocab.enc(t, lower=False))
            ids.append(EOS)
        return torch.frombuffer(ids, dtype=torch.int32).clone()

    stream = to_stream(corpus)
    # 이어서 학습할 때: 절반은 새 문서에서, 절반은 예전 문서 복습 (잊어버리지 않게)
    new_stream = to_stream(new_texts) if sum(map(len, new_texts)) > L * 4 else None

    def lm_batch(n):
        def take(st, k):
            idx = torch.randint(0, len(st) - L - 1, (k,))
            return (torch.stack([st[i:i + L] for i in idx]).long(), torch.stack([st[i + 1:i + L + 1] for i in idx]).long())
        if new_stream is not None and n >= 2:
            (x1, y1), (x2, y2) = take(new_stream, n // 2), take(stream, n - n // 2)
            x, y = torch.cat([x1, x2]), torch.cat([y1, y2])
        else:
            x, y = take(stream, n)
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
        # 보여주기만 하는 부분이라, 여기서 문제가 생겨도 학습한 모델은 꼭 저장되게
        model.eval()
        print(f"\n── {title} ──")
        try:
            for t in TEST[:6]:
                print(f"{t}  →  {model.reply(vocab, t, temperature=0.7)}")
        except Exception as e:
            print(f"(예시 대답을 보여주지 못했습니다: {e})")
        model.train()

    print("\n1단계: 공책 문서로 한국어 배우기")
    run("사전학습", pre_steps, d["lr"] * lr_scale, lambda: loss_of(*lm_batch(d["batch"])))
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
    run("대화학습", sft_steps, d["lr"] * 0.5 * lr_scale,
        lambda: loss_of(*sft_batch(random.choices(dialog, k=d["batch"]))) + 0.2 * loss_of(*lm_batch(d["batch"] // 4)))
    show("학습 결과")

    # 저장 (이전 모델 백업)
    model.eval().cpu().float()
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
    # 이번에 배운 문서 기록 (다음에 이어서 학습할 때 새 문서만 골라내려고)
    with open(os.path.join(out, "arcana-chat.seen"), "w", encoding="utf-8") as f:
        f.write("\n".join(sorted(seen | {text_hash(t) for t in corpus})))
    clean_backups(out, keep=3)
    print(f"\n완료! → {out}\n학습 탭에서 /Reload 를 입력하거나 앱을 다시 켜면 새 모델로 대화합니다.")


if __name__ == "__main__":
    main()

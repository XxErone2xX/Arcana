"""
Arcana 대화 모델 학습

  python convert_dialogue.py 대화.txt       # 먼저: 대화 파일 → data/dialogue.jsonl (대답은 합니다체로)
  python train_chat.py                      # small (약 200만) — CPU 15분
  python train_chat.py --size base          # base  (약 2,500만) — 그래픽카드 권장 (3060 Ti 기준 2~3시간)
  python train_chat.py --size base --data my_chat.jsonl   # 직접 구한 대화 데이터 추가

--data 파일 형식 (한 줄에 대화 하나):
  {"user": "배고파", "bot": "식사하실 시간인가 봅니다. 뭐 드시겠습니까?"}

학습 데이터: data/ 폴더의 모든 .jsonl  +  --data 로 준 파일
  (chat_data.py 템플릿은 Claude가 쓴 문장이라 Anthropic 약관상 대화 모델 학습에 안 씀)

결과: %APPDATA%\\Arcana\\models\\arcana-chat.*  → 학습 탭 /Reload 또는 앱 다시 켜기
(공책 문서로 사전학습까지 하려면 pretrain_notes.py 가 더 좋음)
"""
import argparse
import json
import os
import random
import time
import torch
import torch.nn.functional as F
from chat_data import generate
from chat_model import ChatGPT, CharVocab, PRESETS, USER, BOT, EOS, PAD
from export import write_safetensors

HERE = os.path.dirname(os.path.abspath(__file__))
MODELS = os.path.join(HERE, "..", "models")
DEFAULTS = {  # 크기별 기본 학습량
    "small": dict(steps=2000, batch=64, lr=2e-3, pairs=60000),
    "base": dict(steps=20000, batch=64, lr=6e-4, pairs=300000),
    "large": dict(steps=60000, batch=48, lr=3e-4, pairs=600000),
}
TEST = ["뭐 해?", "나 내일 발표야.", "배고파", "어제 영화 보고 왔어.", "주말에 뭐 했어?", "오늘 너무 춥다.",
        "나 시험 망했어", "심심해", "고마워", "같이 밥 먹을래?", "나 이어폰 잃어버렸어", "오늘 학원 가?",
        "방학 때 뭐 할 거야?", "나 오늘 칭찬받았어", "피곤하다 진짜", "게임 할래?"]
DATA_DIR = os.path.join(HERE, "data")


def load_pairs(extra_files, with_templates, n_templates):
    pairs = []
    files = sorted(os.path.join(DATA_DIR, f) for f in os.listdir(DATA_DIR) if f.endswith(".jsonl")) if os.path.isdir(DATA_DIR) else []
    for p in files + list(extra_files):
        got = load_jsonl(p)
        print(f"데이터 {os.path.relpath(p, HERE)}: {len(got)}개")
        pairs += got
    if with_templates:
        print("⚠ chat_data.py 템플릿 포함 (Claude가 쓴 문장 — 개인 실험용으로만)")
        pairs += generate(n_templates)
    return pairs


def load_jsonl(path):
    out = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                d = json.loads(line)
                if d.get("user") and d.get("bot"):
                    out.append((d["user"], d["bot"]))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--size", choices=PRESETS, default="small")
    ap.add_argument("--steps", type=int)
    ap.add_argument("--batch", type=int)
    ap.add_argument("--lr", type=float)
    ap.add_argument("--data", nargs="*", default=[], help="추가 대화 데이터 (.jsonl)")
    ap.add_argument("--with-templates", action="store_true", help="chat_data.py 템플릿도 섞기 (기본: 안 섞음)")
    args = ap.parse_args()
    d = DEFAULTS[args.size]
    steps, batch, lr = args.steps or d["steps"], args.batch or d["batch"], args.lr or d["lr"]

    torch.manual_seed(0)
    random.seed(0)
    torch.set_num_threads(os.cpu_count() or 2)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    amp = device == "cuda"  # 그래픽카드면 bf16 반정밀도로 (메모리 절반, 속도 2배)

    pairs = load_pairs(args.data, args.with_templates, d["pairs"])
    if not pairs:
        print("학습할 대화 데이터가 없습니다. 먼저 python convert_dialogue.py 대화.txt 를 실행해 주세요.")
        return
    chars = set()
    for u, r in pairs:
        chars.update(u.lower() + r)
    nlu = json.load(open(os.path.join(MODELS, "arcana-nlu.json"), encoding="utf-8"))
    chars.update(c for c in nlu["vocab"] if len(c) == 1)
    vocab = CharVocab(chars)

    model = ChatGPT(len(vocab.itos), **PRESETS[args.size]).to(device)
    L = model.config["max_len"]
    n_params = sum(p.numel() for p in model.parameters())
    print(f"크기 {args.size} / 파라미터 {n_params:,} / 대화 {len(pairs):,}개 / 글자 {len(vocab.itos)}종 / {device}"
          + (" (bf16)" if amp else ""))

    def make(b):
        xs, ys = [], []
        for u, r in b:
            p = [USER] + vocab.enc(u)[: L // 2] + [BOT]
            seq = (p + vocab.enc(r, lower=False) + [EOS])[: L + 1]
            x, y = seq[:-1], seq[1:]
            y = [-100] * (len(p) - 1) + y[len(p) - 1:]  # 사용자 말 부분은 점수 안 매김
            xs.append(x)
            ys.append(y)
        T = max(map(len, xs))
        xs = [x + [PAD] * (T - len(x)) for x in xs]
        ys = [y + [-100] * (T - len(y)) for y in ys]
        return torch.tensor(xs, device=device), torch.tensor(ys, device=device)

    opt = torch.optim.AdamW(model.parameters(), lr=lr, weight_decay=0.05, betas=(0.9, 0.95))
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=lr, total_steps=steps, pct_start=0.05)
    model.train()
    t0, avg = time.time(), None
    for step in range(1, steps + 1):
        x, y = make(random.sample(pairs, batch))
        with torch.autocast(device_type="cuda", dtype=torch.bfloat16, enabled=amp):
            logits = model(x)
        loss = F.cross_entropy(logits.float().reshape(-1, len(vocab.itos)), y.reshape(-1), ignore_index=-100)
        opt.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        opt.step()
        sched.step()
        avg = loss.item() if avg is None else avg * 0.98 + loss.item() * 0.02
        if step % 250 == 0:
            el = time.time() - t0
            print(f"step {step}/{steps}  loss {avg:.3f}  ({el:.0f}초, 남은 시간 약 {el / step * (steps - step) / 60:.0f}분)",
                  flush=True)

    model.eval().cpu().float()
    print("\n── 테스트 ──")
    for t in TEST:
        print(f"{t}  →  {model.reply(vocab, t, temperature=0.7)}")

    from train_feedback import default_dir
    out = os.path.join(default_dir(), "models")
    os.makedirs(out, exist_ok=True)
    torch.save({"config": model.config, "state": model.state_dict(), "vocab": vocab.itos},
               os.path.join(HERE, f"chat_model_{args.size}.pt"))
    export(model, vocab, out, user_texts=[u for u, _ in pairs])
    print("학습 탭에서 /Reload 를 입력하거나 앱을 다시 켜면 새 모델로 대화합니다.")


def known_inputs(user_texts):
    """학습한 사용자 말 목록 → 앱이 "처음 보는 말"인지 판단할 때 씀"""
    import re
    out = {re.sub(r"\s+", " ", u).strip() for u in user_texts}
    return sorted(u for u in out if len(re.sub(r"[\s|]", "", u)) >= 2)


def export(model, vocab, out, user_texts):
    write_safetensors(os.path.join(out, "arcana-chat.safetensors"), model.state_dict())
    meta = {
        "format_version": 1,
        "architecture": "arcana-chat-v1",
        "config": model.config,
        "layer_norm_eps": 1e-5,
        "vocab": vocab.itos,
        "special": {"pad": 0, "unk": 1, "user": 2, "bot": 3, "eos": 4},
        # 앱이 "처음 보는 말인지" 판단할 때 쓰는 학습 문장 목록 (슬롯 자리는 빠짐)
        "known_inputs": known_inputs(user_texts),
    }
    with open(os.path.join(out, "arcana-chat.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=1)
    print(f"내보내기 완료 → {os.path.abspath(out)}")


if __name__ == "__main__":
    main()

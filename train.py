"""
학습 스크립트:  python train.py
- data.py로 데이터를 만들고 model.py의 트랜스포머를 처음부터 학습한다.
- 결과: assistant_model.pt
CPU로도 몇 분이면 끝남. (그래픽카드 있으면 자동으로 사용)
"""
import random
import time
import torch
import torch.nn.functional as F
from data import generate, INTENTS
from model import AssistantModel, CharTokenizer, save_bundle

MAX_LEN = 64
EPOCHS = 6
BATCH = 64
LR = 1e-3


def main():
    torch.manual_seed(0)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    data = generate(n_per_intent=2000)
    split = int(len(data) * 0.95)
    train, val = data[:split], data[split:]

    intents = list(INTENTS.keys())
    tags = ["O"] + [f"{p}-{s}" for s in ("APP", "TIME", "QUERY", "MEMO") for p in ("B", "I")]
    chars = set()
    for t, _, _ in data:
        chars.update(t.lower())
    tok = CharTokenizer(chars)
    print(f"데이터 {len(train)}개 / 검증 {len(val)}개 / 글자 {len(tok.itos)}종 / 장치 {device}")

    def encode_batch(batch):
        ids = [tok.encode(t, MAX_LEN) for t, _, _ in batch]
        tg = [[-100] + [tags.index(x) for x in tg][: MAX_LEN - 1] for _, tg, _ in batch]
        L = max(map(len, ids))
        ids = [x + [0] * (L - len(x)) for x in ids]
        tg = [x + [-100] * (L - len(x)) for x in tg]
        it = [intents.index(i) for _, _, i in batch]
        return torch.tensor(ids, device=device), torch.tensor(tg, device=device), torch.tensor(it, device=device)

    model = AssistantModel(len(tok.itos), len(intents), len(tags), max_len=MAX_LEN).to(device)
    print(f"파라미터 수: {sum(p.numel() for p in model.parameters()):,}")
    opt = torch.optim.AdamW(model.parameters(), lr=LR, weight_decay=0.01)
    steps = EPOCHS * (len(train) // BATCH + 1)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=LR, total_steps=steps, pct_start=0.1)

    for ep in range(EPOCHS):
        model.train()
        random.shuffle(train)
        t0, tot = time.time(), 0
        for i in range(0, len(train), BATCH):
            ids, tg, it = encode_batch(train[i:i + BATCH])
            li, ls = model(ids)
            loss = F.cross_entropy(li, it, label_smoothing=0.05) + \
                F.cross_entropy(ls.reshape(-1, ls.size(-1)), tg.reshape(-1), ignore_index=-100)
            opt.zero_grad()
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            sched.step()
            tot += loss.item()
        # 검증
        model.eval()
        ci = cs = ns = 0
        with torch.no_grad():
            for i in range(0, len(val), 256):
                ids, tg, it = encode_batch(val[i:i + 256])
                li, ls = model(ids)
                ci += (li.argmax(-1) == it).sum().item()
                m = tg != -100
                cs += ((ls.argmax(-1) == tg) & m).sum().item()
                ns += m.sum().item()
        print(f"epoch {ep+1}/{EPOCHS}  loss {tot/(len(train)/BATCH):.4f}  "
              f"의도 정확도 {ci/len(val):.2%}  슬롯 정확도 {cs/ns:.2%}  ({time.time()-t0:.0f}초)")

    save_bundle("assistant_model.pt", model.cpu(), tok, intents, tags)
    print("저장 완료: assistant_model.pt")


if __name__ == "__main__":
    main()

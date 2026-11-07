"""
Rust 엔진 검사용 작은 대화 모델 만들기 (학습 안 한 무작위 모델 — 계산이 PyTorch와 똑같은지만 봄)
  python make_test_fixture.py  →  ../crates/arcana-core/tests/fixtures/
"""
import json, os, torch
from chat_model import ChatGPT, CharVocab
from export import write_safetensors

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "crates", "arcana-core", "tests", "fixtures")
TEXTS = ["안녕하세요", "오늘 날씨", "가나다라마바사", "테스트 문장입니다", "abc xyz 123"]


def main():
    torch.manual_seed(42)
    vocab = CharVocab(set("".join(TEXTS)))
    m = ChatGPT(len(vocab.itos), d_model=32, n_heads=4, n_layers=2, d_ff=64, max_len=48, dropout=0.0)
    with torch.no_grad():  # 값을 크게 해서 1등 글자가 뚜렷하게 (부동소수점 차이로 순위가 안 바뀌게)
        for p in m.parameters():
            p.normal_(0, 0.5)
    m.eval()
    os.makedirs(OUT, exist_ok=True)
    write_safetensors(os.path.join(OUT, "chat-test.safetensors"), m.state_dict())
    meta = {"format_version": 1, "architecture": "arcana-chat-v1", "config": m.config, "layer_norm_eps": 1e-5,
            "vocab": vocab.itos, "special": {"pad": 0, "unk": 1, "user": 2, "bot": 3, "eos": 4}, "known_inputs": []}
    json.dump(meta, open(os.path.join(OUT, "chat-test.json"), "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    golden = [{"text": t, "greedy": m.reply(vocab, t, temperature=0, max_new=20)} for t in TEXTS]
    json.dump(golden, open(os.path.join(OUT, "golden-chat.json"), "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("테스트용 모델 →", os.path.abspath(OUT))
    for g in golden:
        print(g)


if __name__ == "__main__":
    main()

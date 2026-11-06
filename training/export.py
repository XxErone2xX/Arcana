"""
학습된 PyTorch 체크포인트 → Rust 엔진이 읽는 형식으로 내보내기
  python export.py            (train.py 끝나면 자동으로 실행됨)

만들어지는 파일 (../models/):
  arcana-nlu.safetensors  가중치 (HuggingFace safetensors 표준 형식, 직접 작성)
  arcana-nlu.json         모델 설정 + 글자 사전 + 의도/태그 목록
  golden.json             Rust 엔진이 PyTorch랑 같은 답을 내는지 검사하는 테스트 데이터
"""
import json
import os
import struct
import sys
import torch
import torch.nn.functional as F
from model import load_bundle

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "models")
FORMAT_VERSION = 1

GOLDEN_SENTENCES = [
    "디스코드 켜줄래요", "크롬 꺼", "라면 3분 타이머 맞춰줘", "지금 몇시지", "오늘 몇일이야",
    "파이썬 딕셔너리 사용법 검색해줘", "유튜브에서 침착맨 틀어줘", "소리 좀 키워봐", "스샷",
    "내일 체육복 챙기기 메모해줘", "메모 보여줘", "ㅎㅇ", "피자 먹고 싶다", "asdfgh",
    "Chrome 열어", "1시간 30분 뒤에 알려줘", "컴퓨터 꺼줘",
]


def write_safetensors(path, tensors):
    header, offset, blobs = {}, 0, []
    for name, t in tensors.items():
        data = t.detach().contiguous().to(torch.float32).numpy().tobytes()
        header[name] = {"dtype": "F32", "shape": list(t.shape), "data_offsets": [offset, offset + len(data)]}
        offset += len(data)
        blobs.append(data)
    header["__metadata__"] = {"format": "arcana-nlu", "version": str(FORMAT_VERSION)}
    h = json.dumps(header, separators=(",", ":")).encode()
    h += b" " * ((8 - len(h) % 8) % 8)
    with open(path, "wb") as f:
        f.write(struct.pack("<Q", len(h)))
        f.write(h)
        for b in blobs:
            f.write(b)


def main(ckpt=os.path.join(HERE, "assistant_model.pt")):
    os.makedirs(OUT, exist_ok=True)
    model, tok, intents, tags = load_bundle(ckpt)
    write_safetensors(os.path.join(OUT, "arcana-nlu.safetensors"), model.state_dict())

    meta = {
        "format_version": FORMAT_VERSION,
        "architecture": "joint-encoder-v1",
        "config": {k: v for k, v in model.config.items() if k != "dropout"},
        "layer_norm_eps": 1e-5,
        "vocab": tok.itos,
        "intents": intents,
        "tags": tags,
    }
    with open(os.path.join(OUT, "arcana-nlu.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=1)

    golden = []
    with torch.no_grad():
        for s in GOLDEN_SENTENCES:
            ids = tok.encode(s, model.config["max_len"])
            li, ls = model(torch.tensor([ids]))
            golden.append({
                "text": s,
                "intent_probs": [round(x, 6) for x in F.softmax(li[0], -1).tolist()],
                "slot_tags": ls[0].argmax(-1).tolist(),
            })
    with open(os.path.join(OUT, "golden.json"), "w", encoding="utf-8") as f:
        json.dump(golden, f, ensure_ascii=False, indent=1)
    print(f"내보내기 완료 → {os.path.abspath(OUT)}")


if __name__ == "__main__":
    main(*sys.argv[1:])


def read_safetensors(path):
    """write_safetensors 로 만든 파일 읽기 → {이름: 텐서}"""
    import numpy as np
    with open(path, "rb") as f:
        n = struct.unpack("<Q", f.read(8))[0]
        header = json.loads(f.read(n))
        body = f.read()
    out = {}
    for name, h in header.items():
        if name == "__metadata__":
            continue
        a, b = h["data_offsets"]
        arr = np.frombuffer(body[a:b], dtype=np.float32).reshape(h["shape"])
        out[name] = torch.from_numpy(arr.copy())
    return out

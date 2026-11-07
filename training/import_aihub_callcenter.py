"""
AI Hub 콜센터 QA셋 데이터 (다국어 IT 콜센터, 한국어 번역 talk_ko 사용) → 2단계(대화) 학습 데이터

  py import_aihub_callcenter.py "받은 폴더 또는 zip 파일들" ...
  → data/aihub_callcenter.jsonl   (한 줄에 {"user": 고객 말, "bot": 상담사 대답})

- QA셋(CQA_*.json)의 대화에서 "고객 → 상담사" 로 이어지는 말을 한 쌍으로
- 상담사 대답을 Arcana 말투(합니다체)로: "확인해 보세요" → "확인해 보십시오", "있어요" → "있습니다"
  (확실히 못 바꾸는 문장은 해요체 그대로 둠)
- 기계 번역 찌꺼기 정리: "요.?" → "요?", 마침표 뒤 띄어쓰기, 이상한 감탄사("엄청난.") 제거
- 모델이 한 번에 볼 수 있는 길이에 맞게 대답은 앞 문장들만 (최대 약 150자)

※ AI Hub 데이터는 이용약관상 다른 곳에 올리거나 나눠 주면 안 됨 → data/ 는 레포에 안 올라감(.gitignore)
"""
import argparse
import json
import os
import re
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from convert_dialogue import to_formal, batchim, set_jong  # noqa: E402

MAX_REPLY = 150
MAX_USER = 120
JUNK_LEAD = re.compile(r"^(엄청난|훌륭한|완벽한|좋은|대단한|알았다|좋아|오케이|네|예|자)[.!,]\s*")


def tidy(t):
    t = t.replace("​", "").strip()
    t = re.sub(r"\.\?$", "?", t) if re.search(r"(나요|까요|가요|어요|세요|죠|니까)\.\?$", t) else re.sub(r"\.\?$", ".", t)
    t = re.sub(r"([.?!])(?=[가-힣A-Za-z])", r"\1 ", t)  # "보겠습니다.설정" → "보겠습니다. 설정"
    t = re.sub(r"\s{2,}", " ", t)
    return t.strip()


def formal_sentence(s):
    """해요체 문장 하나 → 합니다체 (못 바꾸면 그대로)"""
    core = s.rstrip(".!? ")
    end = s[len(core):].strip() or "."
    q = "?" in end
    if re.search(r"(니다|니까|시오|십시오|군요|지요|죠|까요)$", core):
        return s
    if core.endswith("주세요") and not q:
        return core[:-3] + "주십시오."
    if core.endswith("세요"):
        return core[:-2] + ("시겠습니까?" if q else "십시오.")
    # "했나요?" → "했습니까?", "되나요?" → "됩니까?"
    if core.endswith("나요") and len(core) > 2:
        stem = core[:-2]
        last = stem[-1]
        if batchim(last):
            return stem + "습니까?"
        return stem[:-1] + set_jong(last, "ㅂ") + "니까?"
    # "인가요?" → "입니까?", "이신가요?" → "이십니까?", "건가요?" → "겁니까?"
    if core.endswith("가요") and len(core) > 2 and batchim(core[-3]) == "ㄴ":
        return core[:-3] + set_jong(core[-3], "ㅂ") + "니까?"
    if core.endswith("요"):
        r = to_formal(core[:-1] + ("?" if q else ""))
        if r:
            return r
    return s


def formal(text):
    sents = [x.strip() for x in re.findall(r"[^.?!]+[.?!]*", text) if x.strip()]
    out = " ".join(formal_sentence(x) for x in sents)
    return re.sub(r"시시(?=겠|었|ㅂ|십|ㄹ)", "시", out).replace("시시겠", "시겠")


def first_sentences(text, limit=MAX_REPLY):
    sents = [x.strip() for x in re.findall(r"[^.?!]+[.?!]*", text) if x.strip()]
    out = ""
    for s in sents:
        if out and len(out) + len(s) + 1 > limit:
            break
        out = (out + " " + s).strip()
    return out if len(out) <= limit * 1.3 else ""


def iter_json(path):
    if os.path.isdir(path):
        for root, _, files in os.walk(path):
            for f in files:
                p = os.path.join(root, f)
                if f.endswith(".json"):
                    with open(p, "rb") as fh:
                        yield f, fh.read()
                elif ".zip" in f:
                    yield from iter_json(p)
    elif ".zip" in path:
        try:
            with zipfile.ZipFile(path) as z:
                for n in z.namelist():
                    if n.endswith(".json"):
                        yield os.path.basename(n), z.read(n)
        except zipfile.BadZipFile:
            print(f"  zip 을 열 수 없음 (건너뜀): {path}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--out", default=os.path.join(HERE, "data", "aihub_callcenter.jsonl"))
    ap.add_argument("--keep-style", action="store_true", help="말투(해요체)를 바꾸지 않음")
    args = ap.parse_args()
    seen, rows, files = set(), [], 0
    for p in args.paths:
        for name, raw in iter_json(p):
            try:
                d = json.loads(raw.decode("utf-8-sig"))
            except Exception:
                continue
            turns = d.get("turns") or []
            if not turns or "talk_ko" not in turns[0]:
                continue  # QA셋이 아닌 파일(의도 데이터 등)은 건너뜀
            files += 1
            sp = lambda t: t.get("speaker") or next((v for k, v in t.items() if k.startswith("speaker")), "")
            for a, b in zip(turns, turns[1:]):
                if sp(a) != "고객" or sp(b) != "상담사":
                    continue
                u, r = tidy(a.get("talk_ko", "")), tidy(b.get("talk_ko", ""))
                r = JUNK_LEAD.sub("", r)
                if not args.keep_style:
                    r = formal(r)
                r = first_sentences(r)
                if not u or not r or len(u) > MAX_USER or len(r) < 4:
                    continue
                k = (u, r)
                if k in seen:
                    continue
                seen.add(k)
                rows.append({"user": u, "bot": r})
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"대화 {files:,}개 → 쌍 {len(rows):,}개 → {args.out}")


if __name__ == "__main__":
    main()

"""
AI Hub '계약 외 법률 문서 서식 데이터' → 1단계(사전학습) 재료로 변환

  py import_aihub_legal.py "받은 폴더 또는 zip 파일들" ...
  → corpus/aihub_legal_docs.jsonl   (한 줄에 문서 하나: {"title", "source", "text"})

- zip(.zip, .zip.part0)과 압축을 푼 폴더 둘 다 됨. 하위 폴더까지 전부 찾음
- 라벨링(TL/VL)과 원천(TS/VS)에 같은 문서가 있으면 한 번만
- 개인정보 자리표시자 "(@성명1)" 같은 건 "○○" 로, 자리표시자만 있는 줄은 뺌
- 이건 대화가 아니라 문서라서 1단계(한국어 익히기) 재료로만 씀

※ AI Hub 데이터는 이용약관상 다른 곳에 올리거나 나눠 주면 안 됨 → corpus/ 는 레포에 안 올라감(.gitignore)
"""
import argparse
import io
import json
import os
import re
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
PLACEHOLDER = re.compile(r"\(@[^)]{1,30}\)|\((?:성명|주소|회사명|전화번호|주민등록번호|사건번호)\d*\)")
ONLY_PLACEHOLDER = re.compile(r"^[\s\d.①-⑳()·,:\-]*(\(@[^)]{1,30}\)[\s.,]*)+$")


def doc_text(d):
    doc = d.get("document", d)
    lines = []
    for s in doc.get("sections", []):
        t = ((s.get("content") or {}).get("description") or "").strip()
        for line in t.split("\n"):
            line = line.strip()
            if not line or ONLY_PLACEHOLDER.match(line):
                continue
            line = PLACEHOLDER.sub("○○", line)
            line = re.sub(r"[ \t]{2,}", " ", line)
            # 가려진 곳이 많은 줄(이름·주소 칸 등)은 빼고, 문장 위주로 남김
            if line.count("○○") >= 3 or ("○○" in line and len(line) < 30):
                continue
            lines.append(line)
    return "\n".join(lines)


def iter_json(path):
    """폴더·zip 안의 모든 .json 을 (이름, 내용) 으로"""
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
    elif path.endswith(".json"):
        with open(path, "rb") as fh:
            yield os.path.basename(path), fh.read()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--out", default=os.path.join(HERE, "corpus", "aihub_legal_docs.jsonl"))
    args = ap.parse_args()
    seen, rows, bad = set(), [], 0
    for p in args.paths:
        for name, raw in iter_json(p):
            try:
                d = json.loads(raw.decode("utf-8-sig"))
            except Exception:
                bad += 1
                continue
            meta = ((d.get("document") or {}).get("metadata") or {}).get("file_info") or {}
            title = meta.get("document_name") or os.path.splitext(name)[0]
            if title in seen:
                continue
            text = doc_text(d)
            # 자리표시자투성이(내용 대부분이 ○○)인 서식은 문장 공부에 안 좋아서 뺌
            if len(text) < 100 or text.count("○○") * 2 / len(text) > 0.03:
                continue
            seen.add(title)
            cat = (meta.get("document_category") or {}).get("detail_category", "")
            rows.append({"title": title, "category": cat, "source": "AI Hub 계약 외 법률 문서", "text": text})
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    chars = sum(len(r["text"]) for r in rows)
    print(f"문서 {len(rows):,}개 ({chars:,}글자) → {args.out}" + (f"  (읽지 못한 파일 {bad}개)" if bad else ""))


if __name__ == "__main__":
    sys.exit(main())

"""
위키백과 한 번에 받기: 위키미디어가 공식으로 공개하는 위키백과 전체 덤프를 받아서
문서 본문만 뽑아 1단계(사전학습) 재료로 저장. 사이트를 한 장씩 읽는 것보다 압도적으로 빠름.

  py import_wiki_dump.py              → 한국어 위키백과
  py import_wiki_dump.py ja --max-docs 50000
  py import_wiki_dump.py --file kowiki-latest-pages-articles.xml.bz2   (이미 받은 파일)

  → corpus/wikipedia_ko.jsonl   (한 줄에 {"title", "url", "source", "text"})

- 받는 곳: https://dumps.wikimedia.org/<언어>wiki/latest/<언어>wiki-latest-pages-articles.xml.bz2
  (한국어 약 1GB. 받으면서 바로 풀어 읽기 때문에 디스크에 1GB 를 따로 쓰지 않음)
- 위키 문법(틀, 표, 각주, 그림, 분류 등)은 지우고 문장만 남김
- 위키백과 글은 CC BY-SA 라이선스: 학습에 써도 되고, 나눠 줄 땐 출처를 밝혀야 함
"""
import argparse
import bz2
import json
import os
import re
import sys
import time
import urllib.request
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
UA = "ArcanaAssistant/1.0 (personal assistant; one-time dump download)"

COMMENT = re.compile(r"<!--.*?-->", re.S)
REF = re.compile(r"<ref[^>/]*/>|<ref[^>]*>.*?</ref>", re.S | re.I)
TAGS_DROP = re.compile(r"<(math|gallery|timeline|score|syntaxhighlight|source|pre|code|table|imagemap)[^>]*>.*?</\1>", re.S | re.I)
TAG = re.compile(r"</?[a-zA-Z][^>]*>")
TEMPLATE = re.compile(r"\{\{[^{}]*\}\}")
TABLE = re.compile(r"\{\|.*?\|\}", re.S)
FILE_LINK = re.compile(r"\[\[(?:파일|그림|File|Image|ファイル|画像|분류|Category|カテゴリ):[^\[\]]*(?:\[\[[^\]]*\]\][^\[\]]*)*\]\]", re.I)
LINK = re.compile(r"\[\[(?:[^|\]]*\|)?([^\]]+)\]\]")
EXT = re.compile(r"\[https?://[^\s\]]+\s*([^\]]*)\]")
HEADING = re.compile(r"^=+\s*(.*?)\s*=+\s*$", re.M)
QUOTES = re.compile(r"'{2,5}")
LIST = re.compile(r"^[*#:;]+\s*", re.M)
BLANKS = re.compile(r"\n{3,}")
TAIL = {"같이 보기", "각주", "외부 링크", "참고 문헌", "참고 자료", "출처", "주석", "関連項目", "脚注", "外部リンク", "See also", "References", "External links", "Notes", "Further reading"}


def clean_wikitext(t):
    t = COMMENT.sub("", t)
    t = REF.sub("", t)
    t = TAGS_DROP.sub("", t)
    t = TABLE.sub("", t)
    for _ in range(8):  # 틀 안에 틀: 안쪽부터 여러 번
        n = TEMPLATE.sub("", t)
        if n == t:
            break
        t = n
    t = FILE_LINK.sub("", t)
    t = LINK.sub(r"\1", t)
    t = EXT.sub(r"\1", t)
    t = QUOTES.sub("", t)
    t = TAG.sub("", t)
    out = []
    for line in t.split("\n"):
        m = HEADING.match(line)
        if m:
            if m.group(1).strip() in TAIL:
                break  # 같이 보기·각주 등 목록 부분부터는 버림
            line = m.group(1)
        line = LIST.sub("", line).strip()
        if line.startswith(("|", "!", "{", "}")) or line.lower().startswith(("#redirect", "#넘겨주기")):
            continue
        out.append(line)
    t = "\n".join(out)
    t = BLANKS.sub("\n\n", t)
    return t.strip()


class Progress:
    """받은 크기 보여주기"""

    def __init__(self, f, total):
        self.f, self.total, self.done, self.t0, self.last = f, total, 0, time.time(), 0

    def read(self, n=-1):
        b = self.f.read(n)
        self.done += len(b)
        if time.time() - self.last > 2:
            self.last = time.time()
            mb = self.done / 1e6
            pct = f" ({self.done * 100 / self.total:.0f}%)" if self.total else ""
            print(f"\r  받는 중 {mb:,.0f}MB{pct} · {mb / max(time.time() - self.t0, 1):.1f}MB/s   ", end="", flush=True)
        return b


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("lang", nargs="?", default="ko")
    ap.add_argument("--file", help="이미 받은 덤프 파일 (.xml.bz2)")
    ap.add_argument("--max-docs", type=int, default=150_000, help="저장할 최대 문서 수")
    ap.add_argument("--min-chars", type=int, default=300, help="이보다 짧은 문서는 건너뜀")
    ap.add_argument("--out")
    args = ap.parse_args()
    out = args.out or os.path.join(HERE, "corpus", f"wikipedia_{args.lang}.jsonl")
    os.makedirs(os.path.dirname(out), exist_ok=True)

    if args.file:
        raw = open(args.file, "rb")
        total = os.path.getsize(args.file)
    else:
        url = f"https://dumps.wikimedia.org/{args.lang}wiki/latest/{args.lang}wiki-latest-pages-articles.xml.bz2"
        print(f"위키백과({args.lang}) 전체 덤프를 받습니다: {url}")
        resp = urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": UA}), timeout=60)
        raw = resp
        total = int(resp.headers.get("Content-Length") or 0)
    stream = bz2.BZ2File(Progress(raw, total))

    saved = skipped = chars = 0
    ns = ""
    with open(out, "w", encoding="utf-8") as f:
        title = text = page_ns = None
        redirect = False
        for event, el in ET.iterparse(stream, events=("start", "end")):
            tag = el.tag.rsplit("}", 1)[-1]
            if event == "start":
                if tag == "page":
                    title, text, page_ns, redirect = None, None, None, False
                continue
            if tag == "title":
                title = el.text
            elif tag == "ns":
                page_ns = el.text
            elif tag == "redirect":
                redirect = True
            elif tag == "text":
                text = el.text or ""
            elif tag == "page":
                if page_ns == "0" and not redirect and title and text:
                    body = clean_wikitext(text)
                    if len(body) >= args.min_chars:
                        url = f"https://{args.lang}.wikipedia.org/wiki/{title.replace(' ', '_')}"
                        f.write(json.dumps({"title": title, "url": url, "source": "위키백과 덤프", "text": body}, ensure_ascii=False) + "\n")
                        saved += 1
                        chars += len(body)
                        if saved % 2000 == 0:
                            print(f"\n  문서 {saved:,}개 저장 ({chars:,}글자)", flush=True)
                    else:
                        skipped += 1
                el.clear()  # 메모리 절약
                if saved >= args.max_docs:
                    break
    print(f"\n완료! 문서 {saved:,}개 ({chars:,}글자) → {out}  (짧거나 빈 문서 {skipped:,}개 건너뜀)")
    print("다음 학습(/Train 또는 자동 학습) 때 1단계 재료로 쓰입니다.")


if __name__ == "__main__":
    sys.exit(main())

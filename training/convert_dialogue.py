"""
대화 데이터 → Arcana 학습 데이터 변환기

  python convert_dialogue.py 대화.txt                  → data/dialogue.jsonl
  python convert_dialogue.py 대화.txt --keep-casual    (말투 변환 안 함)

입력 형식 (빈 줄로 대화 구분):
  A: 뭐 해?
  B: 유튜브 보고 있어.
  A: 나도 심심해.

하는 일:
  - 대화 안의 이어진 두 말을 (사용자 말, 대답) 쌍으로 만듦  (A→B, B→A 모두)
  - 대답만 반말 → 합니다체로 바꿈 ("떨리겠다" → "떨리겠습니다", "뭐 봤어?" → "무엇을 보셨습니까?")
  - 확실하게 못 바꾸는 대답은 빼 버림 (어색한 말투를 배우지 않게)
  - 같은 쌍은 하나만 남김
"""
import argparse
import json
import os
import re
import sys

# ───────────────────────── 한글 자모 도구 ─────────────────────────
CHO = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ"
JUNG = "ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ"
JONG = ["", "ㄱ", "ㄲ", "ㄳ", "ㄴ", "ㄵ", "ㄶ", "ㄷ", "ㄹ", "ㄺ", "ㄻ", "ㄼ", "ㄽ", "ㄾ", "ㄿ", "ㅀ", "ㅁ", "ㅂ", "ㅄ",
        "ㅅ", "ㅆ", "ㅇ", "ㅈ", "ㅊ", "ㅋ", "ㅌ", "ㅍ", "ㅎ"]


def is_syl(c):
    return "가" <= c <= "힣"


def split(c):
    n = ord(c) - 0xAC00
    return CHO[n // 588], JUNG[(n % 588) // 28], JONG[n % 28]


def join(cho, jung, jong=""):
    return chr(0xAC00 + CHO.index(cho) * 588 + JUNG.index(jung) * 28 + JONG.index(jong))


def batchim(c):
    return split(c)[2] if is_syl(c) else ""


def set_jong(c, jong):
    cho, jung, _ = split(c)
    return join(cho, jung, jong)


# ───────────────────────── 어간 → 합니다체 ─────────────────────────
SPECIAL_HON = {"먹": "드시", "자": "주무시", "있": "계시", "마시": "드시"}  # 높임 특수형 (질문용)


def formal_from_stem(stem, question=False, honor=False):
    """'가' → '갑니다', '먹' → '먹습니다', '알' → '압니다' (질문이면 '…니까')"""
    end = "니까" if question else "니다"
    if honor:
        last2 = stem[-2:]
        if stem in SPECIAL_HON or last2 in SPECIAL_HON:
            key = stem if stem in SPECIAL_HON else last2
            stem = stem[: len(stem) - len(key)] + SPECIAL_HON[key]
        else:
            b = batchim(stem[-1])
            if b == "ㄹ":
                stem = stem[:-1] + set_jong(stem[-1], "")
                b = ""
            stem = stem + ("으시" if b else "시")
    last = stem[-1]
    b = batchim(last)
    if b == "ㄹ":  # 알 → 압니다, 들 → 듭니다
        return stem[:-1] + set_jong(last, "ㅂ") + end
    if b:
        return stem + "습" + end
    return stem[:-1] + set_jong(last, "ㅂ") + end


# 'ㅂ' 불규칙 형용사 (어려워 → 어렵다)
B_IRREG = {"어려워": "어렵", "귀여워": "귀엽", "추워": "춥", "더워": "덥", "무서워": "무섭", "쉬워": "쉽", "반가워": "반갑",
           "즐거워": "즐겁", "아까워": "아깝", "가까워": "가깝", "부끄러워": "부끄럽", "매워": "맵", "외로워": "외롭",
           "부러워": "부럽", "무거워": "무겁", "가벼워": "가볍", "시끄러워": "시끄럽", "뜨거워": "뜨겁", "차가워": "차갑",
           "아름다워": "아름답", "놀라워": "놀랍", "괴로워": "괴롭", "그리워": "그립", "새로워": "새롭"}
# 'ㄷ' 불규칙 (들어 → 듣다)
D_IRREG = {"들어": "듣", "걸어": "걷", "물어": "묻", "깨달아": "깨닫"}
# 'ㅓ' 탈락 (써 → 쓰다)
EO_DROP = {"써": "쓰", "커": "크", "꺼": "끄", "떠": "뜨", "퍼": "푸"}
# 'ㅡ' 탈락 (아파 → 아프다)
EU_DROP = {"아파": "아프", "바빠": "바쁘", "고파": "고프", "예뻐": "예쁘", "나빠": "나쁘", "슬퍼": "슬프", "기뻐": "기쁘",
           "배고파": "배고프", "모아": "모으", "따라": "따르"}
# 받침 없는 말이 그대로 어간인 동사 (끝나, 가 …). 이 목록에 없으면 명사일 수 있어서 안 바꿈
BARE_VERBS = ("끝나", "생각나", "기억나", "만나", "일어나", "나가", "들어가", "올라가", "내려가", "돌아가", "지나", "보내",
              "끝내", "지내", "떠나", "자라", "모자라", "잘나가", "놀라")
BARE_ONE = {"나", "가", "자", "사", "타", "서", "내", "차", "싸"}  # 한 글자일 때만 (학교가 ← 조사 '가'와 구분)
# 말이 조사로 끝나면 문장이 아직 안 끝난 것 (중순부터, 시에) → 안 바꿈
PARTICLES = ("부터", "까지", "에서", "에게", "한테", "으로", "처럼", "보다", "에", "랑", "하고", "만", "도", "로", "의")
QUESTION_WORDS = ("왜", "뭐", "무슨", "어디", "언제", "누구", "누가", "몇", "어떻게", "어때", "얼마")
LEAD = {"오": "", "아": "", "와": "와,", "헐": "", "대박": "정말", "우와": "와,", "음": "", "앗": "", "ㅇㅋ": "네,", "오케이": "네,", "콜": "네,",
        "그래": "네,", "그래,": "네,", "응": "네,", "알겠어": "알겠습니다,", "알았어": "알겠습니다,", "좋아": "좋습니다,",
        "고마워": "감사합니다,", "미안": "죄송합니다,", "진짜": "정말", "ㅇㅇ": "네,", "웅": "네,", "어": "네,", "아니": "아니요,", "아니,": "아니요,"}


def stem_of_haeche(word):
    """해체 현재형 → 어간.  가→가, 알아→알, 해→하, 봐→보, 줘→주, 졸려→졸리, 골라→고르, 먹어→먹"""
    for k, v in {**B_IRREG, **D_IRREG}.items():
        if word.endswith(k):
            return word[: -len(k)] + v
    for k, v in {**EO_DROP, **EU_DROP}.items():
        if word.endswith(k):
            return word[: -len(k)] + v
    last = word[-1]
    if not is_syl(last):
        return None
    cho, jung, jong = split(last)
    if jong:
        return None
    pre = word[:-1]
    if last in ("아", "어") and pre:
        return pre  # 알아 → 알, 먹어 → 먹
    if last in ("라", "러") and pre and batchim(pre[-1]) == "ㄹ":  # 르 불규칙: 골라 → 고르
        return pre[:-1] + set_jong(pre[-1], "") + "르"
    if last == "해":
        return pre + "하"
    m = {"ㅘ": "ㅗ", "ㅝ": "ㅜ", "ㅙ": "ㅚ", "ㅕ": "ㅣ"}
    if jung in m:
        return pre + join(cho, m[jung])
    last_word = word.split()[-1] if " " in word else word
    if jung in ("ㅏ", "ㅐ", "ㅓ") and (word.endswith(BARE_VERBS) or last_word in BARE_ONE):
        return word  # 가, 자, 끝나, 내, 서
    return None


def past_stem(word):
    """'했어' → '하', '봤어' → '보', '먹었어' → '먹' (높임 과거 만들 때)"""
    body = word[:-1]  # '어' 떼기
    p = body[-1]
    pre = body[:-1]
    if p in ("었", "았", "였") and pre:
        return pre
    cho, jung, jong = split(p)
    if jong != "ㅆ":
        return None
    if p == "했":
        return pre + "하"
    m = {"ㅘ": "ㅗ", "ㅝ": "ㅜ", "ㅙ": "ㅚ", "ㅕ": "ㅣ", "ㅏ": "ㅏ", "ㅓ": "ㅡ", "ㅐ": "ㅐ"}
    if jung not in m:
        return None
    return pre + join(cho, m[jung])


def honor_past(stem):
    if stem in SPECIAL_HON or stem[-2:] in SPECIAL_HON:
        key = stem if stem in SPECIAL_HON else stem[-2:]
        return stem[: len(stem) - len(key)] + SPECIAL_HON[key] + "셨"
    b = batchim(stem[-1])
    if b == "ㄹ":
        return stem[:-1] + set_jong(stem[-1], "") + "셨"
    return stem + ("으셨" if b else "셨")


# ───────────────────────── 문장 단위 ─────────────────────────
WHOLE = {
    "고마워": "감사합니다", "고마워요": "감사합니다", "땡큐": "감사합니다", "고맙다": "감사합니다",
    "미안": "죄송합니다", "미안해": "죄송합니다", "미안해요": "죄송합니다",
    "축하해": "축하드립니다", "축하해요": "축하드립니다",
    "응": "네", "ㅇㅇ": "네", "웅": "네", "어": "네", "넹": "네", "네": "네",
    "ㅇㅋ": "알겠습니다", "오케이": "알겠습니다", "알겠어": "알겠습니다", "알았어": "알겠습니다", "콜": "좋습니다",
    "그래": "그러시죠", "좋아": "좋습니다",
    "아니": "아니요", "아니야": "아닙니다", "ㄴㄴ": "아니요",
    "진짜": "정말입니까", "정말": "정말입니까", "헐": "정말입니까", "대박": "대단합니다",
    "그렇구나": "그렇군요", "그치": "그렇지요", "그러게": "그러게 말입니다",
    "뭐야": "무엇입니까", "뭔데": "무엇입니까", "왜": "왜 그러십니까", "어때": "어떻습니까",
    "잘 자": "안녕히 주무십시오", "잘자": "안녕히 주무십시오", "힘내": "힘내십시오", "안녕": "안녕하세요",
    "괜찮아": "괜찮습니다", "몰라": "모르겠습니다", "글쎄": "글쎄요", "맞아": "맞습니다", "당연하지": "당연하지요",
}
# 명령으로 쓰이는 말들 (봐 → 보십시오)
# (가, 해, 와처럼 평서문과 헷갈리는 말은 일부러 뺌: "나 학원 가" ≠ 명령)
IMPERATIVE = {"봐", "해봐", "가봐", "들어봐", "물어봐", "말해", "연락해", "쉬어", "챙겨", "불러", "골라",
              "보여줘", "보내줘", "빌려봐", "연습해봐", "봐봐", "기다려", "골라줘", "알려줘", "다녀", "다녀와", "조심해"}
IMP_SPECIAL = {"먹어": "드십시오", "말해": "말씀하십시오", "자": "주무십시오", "봐봐": "보십시오", "와": "오십시오",
               "다녀": "다니십시오", "다녀와": "다녀오십시오",
               "가": "가십시오", "들어": "들어 보십시오"}
# 줘/줄게 → 드리다 (상대를 위해 해 주는 말)
GIVE = [("줄게", "드리겠습니다"), ("줄까", "드릴까요"), ("줘", "주십시오")]
PRONOUN = [(r"^나는$|^난$", "저는"), (r"^내가$", "제가"), (r"^나도$", "저도"), (r"^나$", "저"), (r"^내$", "제"),
           (r"^나한테$", "저한테"), (r"^나랑$", "저랑")]
DROP_WORDS = {"너", "너는", "넌", "너도", "네가", "니가", "야", "ㅋㅋ", "ㅋㅋㅋ", "ㅎㅎ", "ㅠㅠ", "ㅜㅜ"}
CASUAL_WORDS = {"진짜": "정말", "엄청": "아주", "되게": "아주", "완전": "정말", "걔": "그분", "얘": "이분"}
NOISE = re.compile(r"[ㅋㅎㅠㅜ]{2,}|[~]+")


class Fail(Exception):
    pass


def convert_last(w, q):
    """문장 마지막 말 바꾸기. q = 질문인지"""
    if w == "왜":
        return "왜 그러십니까"
    for a, b in (("고마워", "감사합니다"), ("고맙다", "감사합니다"), ("미안해", "죄송합니다"), ("미안", "죄송합니다"),
                 ("축하해", "축하드립니다")):
        if w.endswith(a):
            return w[: -len(a)] + b
    if w.endswith(PARTICLES) and not w.endswith(("하고",)) or w.endswith("하고") and len(w) > 2:
        raise Fail
    for tail in ("는데", "은데"):  # 봤는데? → 보셨습니까? / 괜찮은데 → 괜찮습니다
        if w.endswith(tail) and len(w) > 2:
            pre = w[:-2]
            if batchim(pre[-1]) == "ㅆ":
                return convert_last(pre + "어", q)
            stem = pre
            if stem.endswith(("있", "없", "같", "괜찮", "좋", "싫", "많")) or not q:
                return formal_from_stem(stem, q)
            return formal_from_stem(stem, q, honor=True)
    if w.endswith("데") and len(w) > 1 and batchim(w[-2]) == "ㄴ":  # 큰데 → 큽니다
        stem = w[:-2] + set_jong(w[-2], "")
        return formal_from_stem(stem, q)
    if w.endswith(("습니다", "니다", "니까", "습니까")) or w.endswith("까요"):
        return w
    if w.endswith("세요"):
        return w[:-2] + ("십니까" if q else "십시오")
    if w.endswith(("게요",)):  # 도와드릴게요 → 도와드리겠습니다
        w = w[:-1]
    if w.endswith(("이에요", "예요")):
        return w[: -3 if w.endswith("이에요") else -2] + ("입니까" if q else "입니다")
    if w.endswith("요") and len(w) > 1:
        w = w[:-1]  # 해요체 → 해체로 돌려서 아래 규칙 적용
    for a, b in GIVE:
        if w.endswith(a):
            pre = w[: -len(a)]
            if not pre:
                raise Fail
            return pre + (" " if not pre.endswith(" ") else "") + b
    if not q and w in IMPERATIVE:
        if w in IMP_SPECIAL:
            return IMP_SPECIAL[w]
        if w.endswith("봐") and len(w) > 1:
            return w[:-1] + " 보십시오"
        stem = stem_of_haeche(w)
        if stem:
            b = batchim(stem[-1])
            if b == "ㄹ":
                stem = stem[:-1] + set_jong(stem[-1], "")
                b = ""
            return stem + ("으십시오" if b else "십시오")
    if w.endswith("잖아"):
        return w[:-2] + "지 않습니까"
    if w.endswith("려고"):
        return w + " 합니다"
    if w.endswith(("더라", "더라고")):
        return w[: w.rindex("더라")] + "더군요"
    if w.endswith("구나"):
        return w[:-2] + "군요"
    if w.endswith("네") and len(w) > 1 and not w.endswith("이네"):
        return w[:-1] + "군요"
    if w.endswith("거야"):
        return w[:-2] + ("겁니까" if q else "겁니다")
    if w.endswith("거"):
        return w + ("입니까" if q else "입니다")
    if w.endswith("이야"):
        return w[:-2] + ("입니까" if q else "입니다")
    if w.endswith("야") and len(w) > 1 and not batchim(w[-2]):
        return w[:-1] + ("입니까" if q else "입니다")
    if w.endswith("래") and len(w) > 1:
        pre = w[:-1]
        if q and batchim(pre[-1]) == "ㄹ":  # 갈래? → 가시겠습니까?
            stem = pre[:-1] + set_jong(pre[-1], "")
            if stem.endswith("으"):
                stem = stem[:-1]
                return honor_stem(stem) + "겠습니까"
            return honor_stem(stem) + "겠습니까"
        if pre.endswith("이"):  # 맛이래 → 맛이라고 합니다
            return pre + "라고 합니다"
        raise Fail
    if w.endswith("대") and len(w) > 1 and not q:  # 생겼대 → 생겼다고 합니다
        return w[:-1] + "다고 합니다"
    if w.endswith("게") and len(w) > 1 and batchim(w[-2]) == "ㄹ":  # 갈게 → 가겠습니다
        pre = w[:-1]
        stem = pre[:-1] + set_jong(pre[-1], "")
        if stem.endswith("으"):
            stem = stem[:-1]
        return stem + "겠습니다"
    if w.endswith("자") and len(w) > 1:  # 가자 → 가시죠
        stem = w[:-1]
        if stem.endswith("먹"):
            return stem[:-1] + "드시죠"
        b = batchim(stem[-1])
        if b and b != "ㄹ":
            raise Fail
        if b == "ㄹ":
            stem = stem[:-1] + set_jong(stem[-1], "")
        return stem + "시죠"
    if w.endswith("까") and len(w) > 1:
        return w + "요"
    if w.endswith("지") and len(w) > 1:
        if w == "알지":
            return "아시지요" if q else "알지요"
        return w + "요"
    if w.endswith("다") and len(w) > 1:
        pre = w[:-1]
        if pre.endswith("이") and len(pre) > 1:
            return pre[:-1] + ("입니까" if q else "입니다")
        last = pre[-1]
        b = batchim(last)
        if pre.endswith("는"):  # 먹는다 → 먹습니다
            return formal_from_stem(pre[:-1], q)
        if b == "ㄴ":  # 간다 → 갑니다, 안다 → 압니다
            return pre[:-1] + set_jong(last, "ㅂ") + ("니까" if q else "니다")
        return formal_from_stem(pre, q)
    if w.endswith("어") and len(w) > 1 and batchim(w[-2]) == "ㅆ":  # 했어, 봤어, 겠어, 있어
        if q and w[-2] not in ("겠", "있"):
            stem = past_stem(w)
            if stem:
                return honor_past(stem) + "습니까"
        return w[:-1] + ("습니까" if q else "습니다")
    stem = stem_of_haeche(w)
    if stem and is_syl(stem[-1]):
        honor = q and (stem[-1:] in ("하", "가", "오", "보", "먹", "자", "알", "사", "읽", "쓰", "놀") or stem.endswith(("마시", "배우", "만나", "아프")))
        return formal_from_stem(stem, q, honor=honor)
    raise Fail


def honor_stem(stem):
    if stem.endswith("먹"):
        return stem[:-1] + "드시"
    b = batchim(stem[-1])
    return stem + ("으시" if b else "시")


def to_formal(text):
    """반말 대답 → 합니다체. 못 바꾸면 None"""
    text = NOISE.sub("", text).strip()
    if not text:
        return None
    key = re.sub(r"[.!?\s]", "", text)
    if key in WHOLE:
        q = text.endswith("?") or WHOLE[key].endswith("니까")
        out = WHOLE[key]
        return out + ("?" if out.endswith(("니까", "까요", "지요")) and q else ".")
    out = []
    for sent in re.findall(r"[^.!?]+[.!?]*", text):
        s = sent.strip()
        if not s:
            continue
        core = s.rstrip(".!?").strip()
        q = s.endswith("?") or any(w in QUESTION_WORDS for w in core.split())
        # 문장 앞 감탄사·맞장구 정리 (오, 헐, 응, ㅇㅋ …)
        words0 = core.split()
        lead = ""
        while words0 and words0[0].rstrip(",") in LEAD and len(words0) > 1:
            lead = LEAD[words0[0].rstrip(",")] or lead
            words0 = words0[1:]
        core = " ".join(words0)
        k = core.replace(" ", "")
        if k in WHOLE:
            out.append((lead + " " if lead else "") + WHOLE[k] + ("?" if WHOLE[k].endswith(("니까", "까요", "지요")) else "."))
            continue
        words = core.split()
        # 대명사·호칭·군말 정리
        fixed = []
        for i, wd in enumerate(words):
            if wd in DROP_WORDS:
                continue
            for pat, rep in PRONOUN:
                if re.match(pat, wd):
                    wd = rep
                    break
            fixed.append(CASUAL_WORDS.get(wd, wd))
        if lead:
            fixed.insert(0, lead)
        if not fixed:
            continue
        # "내일 봐", "이따 보자" = 헤어질 때 인사
        if fixed[-1] in ("봐", "보자") and any(t in core for t in ("내일", "이따", "나중에", "다음에", "주말에", "그때", "월요일", "학교에서")):
            fixed[-1] = "뵙겠습니다"
            out.append(" ".join(fixed) + ".")
            continue
        try:
            fixed[-1] = convert_last(fixed[-1], q)
        except Fail:
            return None
        sent2 = " ".join(fixed)
        out.append(sent2 + ("?" if q else "." if not sent2.endswith((".", "!")) else ""))
    res = " ".join(out).strip()
    if not res or re.search(r"(어|아|야|해|지|자|래|게|다|대)[.?!]?$", res.split()[-1]) and not re.search(r"(니다|니까|요|죠|시오|군요)[.?!]?$", res):
        return None
    return res


# ───────────────────────── 대화 파일 ─────────────────────────
def read_dialogues(path):
    txt = open(path, encoding="utf-8-sig").read().replace("\r\n", "\n")
    out = []
    for block in re.split(r"\n\s*\n", txt):
        d = []
        for line in block.split("\n"):
            m = re.match(r"^\s*([^:：]{1,12})\s*[:：]\s*(.+)$", line)
            if m:
                d.append((m.group(1).strip(), m.group(2).strip()))
        if len(d) >= 2:
            out.append(d)
    return out


# 앞 대화를 봐야 뜻이 정해지는 짧은 맞장구 → 이걸 사용자 말로 쓰는 쌍은 뺌 ("고마워" 다음 말은 상황마다 다름)
CONTEXT_ONLY = {"고마워", "고마워요", "응", "ㅇㅇ", "어", "웅", "ㅇㅋ", "오케이", "그래", "그래?", "진짜", "진짜?", "정말?", "헐",
                "대박", "ㅋㅋ", "ㅋㅋㅋ", "왜", "왜?", "뭐", "뭐?", "아니", "아니야", "알겠어", "알았어", "좋아", "그치", "맞아",
                "오", "아", "와", "그렇구나", "뭔데", "뭔데?", "어떤 거?", "갑자기?", "어디서?", "언제?", "누구?", "미안", "괜찮아"}


def context_only(u):
    k = re.sub(r"[\s.!?~]", "", u)
    return k in {re.sub(r"[\s.!?~]", "", c) for c in CONTEXT_ONLY} or len(k) <= 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("inputs", nargs="+")
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "data", "dialogue.jsonl"))
    ap.add_argument("--keep-casual", action="store_true", help="말투 변환 안 함")
    args = ap.parse_args()
    pairs, total, failed = [], 0, []
    seen = set()
    for path in args.inputs:
        for d in read_dialogues(path):
            for (s1, u), (s2, r) in zip(d, d[1:]):
                if s1 == s2 or context_only(u):
                    continue
                total += 1
                bot = r if args.keep_casual else to_formal(r)
                if not bot:
                    failed.append(r)
                    continue
                key = (u, bot)
                if key not in seen:
                    seen.add(key)
                    pairs.append({"user": u, "bot": bot})
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        for p in pairs:
            f.write(json.dumps(p, ensure_ascii=False) + "\n")
    ok = total - len(failed)
    print(f"대화 쌍 {total}개 중 {ok}개 변환 ({ok / max(total, 1):.0%}) → 중복 빼고 {len(pairs)}개 저장: {args.out}")
    if failed:
        from collections import Counter
        print("못 바꾼 대답 예시:", [x for x, _ in Counter(failed).most_common(15)])


if __name__ == "__main__":
    main()

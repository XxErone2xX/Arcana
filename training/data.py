"""
학습 데이터 생성기
- 문장 템플릿 + 슬롯 값을 랜덤 조합해서 (문장, 글자별 태그, 의도) 데이터를 만든다.
- 태그는 BIO 방식: B-APP(앱 이름 시작), I-APP(앱 이름 이어짐), O(해당 없음)
- 새 명령을 추가하고 싶으면 INTENTS에 템플릿을 추가하고 다시 학습(train.py)하면 된다.
"""
import random
import re

# ───────────────────────── 슬롯 값 ─────────────────────────
APPS = [
    "크롬", "chrome", "인터넷", "메모장", "계산기", "디스코드", "디코", "로블록스",
    "탐색기", "파일 탐색기", "스포티파이", "그림판", "작업 관리자", "작업관리자",
    "vscode", "비주얼 스튜디오 코드", "카톡", "카카오톡", "스팀", "엣지", "설정",
    "명령 프롬프트", "cmd", "파워쉘", "로블록스 스튜디오", "블렌더", "오토핫키",
]

MEMOS = [
    "우유 사기", "숙제 하기", "내일 발표 준비", "엄마한테 전화하기", "수학 학원 가기",
    "택배 찾기", "스크립트 버그 고치기", "책 반납하기", "로블록스 업데이트 확인",
    "준비물 챙기기", "충전기 가져가기", "8시에 디코 접속", "독후감 쓰기",
    "친구 생일 선물 사기", "방 청소", "영어 단어 외우기", "물 마시기",
    "블렌더 이펙트 마무리", "케이블 책가방 발표 자료", "과학 수행평가",
]

QUERIES = [
    "날씨", "오늘 날씨", "파이썬 리스트 정렬", "로블록스 루아 문법", "맛집", "환율",
    "블렌더 단축키", "뉴스", "롤 패치노트", "강아지 키우는 법", "고양이 사료 추천",
    "오토핫키 사용법", "축구 결과", "시험 공부 방법", "라면 맛있게 끓이는 법",
    "그래픽카드 가격", "트랜스포머 모델", "인공지능", "서울 지하철 노선도",
    "마인크래프트 서버", "번역기", "아이유", "뉴진스", "로파이 음악", "롤 하이라이트",
    "피아노 연주", "공부할 때 듣는 노래", "웃긴 영상", "요리 레시피",
]

SONGS = [
    "아이유 노래", "뉴진스", "로파이", "잔잔한 노래", "신나는 노래", "공부할 때 듣는 음악",
    "브금", "피아노 음악", "롤 하이라이트", "마크 영상", "고양이 영상", "재즈",
    "운동할 때 듣는 노래", "애니 노래", "에픽 음악", "빗소리",
]

# 처음 보는 단어도 슬롯으로 뽑을 수 있게, 랜덤 단어를 섞어서 학습시킨다
NOUNS = [
    "체육복", "교과서", "급식", "피자", "치킨", "게임", "영상", "노트북", "키보드", "마우스", "모니터",
    "학교", "학원", "숙제", "시험", "발표", "친구", "생일", "선물", "엄마", "아빠", "동생", "강아지",
    "고양이", "택배", "버스", "지하철", "축구", "농구", "야구", "음악", "노래", "가수", "드라마", "영화",
    "애니", "웹툰", "만화", "스크립트", "코딩", "파이썬", "루아", "자바", "서버", "봇", "매크로",
    "이펙트", "모델링", "텍스처", "날씨", "환율", "주식", "비트코인", "뉴스", "레시피", "라면", "떡볶이",
    "편의점", "카페", "도서관", "병원", "약국", "공원", "여행", "일본", "오사카", "도쿄", "제주도",
    "침착맨", "우왁굳", "페이커", "손흥민", "BTS", "아이브", "르세라핌", "마인크래프트", "발로란트",
    "배그", "메이플", "피파", "포켓몬", "젤다", "원신", "lofi", "minecraft", "python", "roblox",
    "lua", "blender", "unity", "discord bot", "youtube", "tiktok", "선풍기", "에어컨", "충전기",
    "체육", "국어", "수학", "영어", "과학", "사회", "음악실", "보고서", "독후감", "수행평가",
]
VERBS_GI = ["하기", "사기", "챙기기", "보내기", "가기", "끝내기", "확인하기", "연락하기", "준비하기",
            "제출하기", "고치기", "만들기", "찾기", "반납하기", "예약하기", "정리하기", "외우기", "보기"]
QUERY_TAILS = ["", "", "", " 사용법", " 추천", " 가격", " 방법", " 뜻", " 순위", " 하이라이트",
               " 공략", " 영상", " 노래", " 리뷰", " 만드는 법", " 최신", " 정리"]
SYL = list("가나다라마바사아자차카타파하고노도로모보소오조초코토포호구누두루무부수우주추쿠투푸후"
           "기니디리미비시이지치키티피히강난단랑망방상앙장창캉탕팡항민진현준성영수철")


def rand_word():
    if random.random() < 0.15:
        return "".join(random.choice("abcdefghijklmnoprstuvwxyz") for _ in range(random.randint(3, 8)))
    return "".join(random.choice(SYL) for _ in range(random.randint(2, 4)))


def rand_query():
    r = random.random()
    if r < 0.35:
        return random.choice(QUERIES + SONGS)
    words = [random.choice(NOUNS) if random.random() < 0.75 else rand_word()
             for _ in range(random.choice([1, 1, 2, 2, 3]))]
    return " ".join(words) + random.choice(QUERY_TAILS)


def rand_memo():
    r = random.random()
    if r < 0.35:
        return random.choice(MEMOS)
    pre = random.choice(["", "", "내일 ", "오늘 ", "주말에 ", "저녁에 ", "학교에서 ", f"{random.randint(1,12)}시에 "])
    obj_ = " ".join(random.choice(NOUNS) if random.random() < 0.8 else rand_word()
                    for _ in range(random.choice([1, 1, 2])))
    return pre + obj_ + " " + random.choice(VERBS_GI)


NUM_SINO =["일", "이", "삼", "사", "오", "육", "칠", "팔", "구", "십", "이십", "삼십", "사십", "오십"]
NUM_NATIVE = ["한", "두", "세", "네", "다섯"]


def rand_duration():
    r = random.random()
    if r < 0.45:
        return f"{random.choice([1,2,3,5,10,15,20,25,30,40,45,50,60,90])}분"
    if r < 0.6:
        return f"{random.choice([10,15,20,30,40,45,50,60,90])}초"
    if r < 0.7:
        return f"{random.choice([1,2,3])}시간"
    if r < 0.8:
        return f"{random.choice([1,2,3,5,10])}분 {random.choice([10,20,30])}초"
    if r < 0.85:
        return f"{random.choice([1,2])}시간 {random.choice([10,20,30])}분"
    if r < 0.93:
        return f"{random.choice(NUM_SINO)}분"
    return f"{random.choice(NUM_NATIVE)}시간"


# ───────────────────────── 조사 처리 ─────────────────────────
def has_batchim(word):
    ch = word.strip()[-1]
    if "가" <= ch <= "힣":
        return (ord(ch) - 0xAC00) % 28 != 0
    return ch.lower() in "lmnr"  # 영어는 대충


# 지식 질문 주제 ("블랙홀이 뭐야" 의 블랙홀)
TOPICS = [
    "블랙홀", "세종대왕", "광합성", "인공지능", "비트코인", "이순신", "아인슈타인", "상대성 이론", "대한민국",
    "일본", "미국", "태양계", "화성", "공룡", "피라미드", "양자역학", "DNA", "민주주의", "조선", "고려",
    "임진왜란", "지구 온난화", "빅뱅", "원자", "전기", "자석", "화산", "지진", "태풍", "무지개", "은하수",
    "달", "태양", "목성", "토성", "바이러스", "세균", "백신", "뉴턴", "에디슨", "레오나르도 다빈치",
    "모나리자", "에펠탑", "만리장성", "올림픽", "월드컵", "축구", "야구", "체스", "바둑", "피아노",
    "트랜스포머", "신경망", "머신러닝", "리눅스", "윈도우", "자바스크립트", "러스트", "루아", "HTML",
    "GPU", "CPU", "반도체", "인터넷", "와이파이", "블루투스", "유튜브", "로블록스", "마인크래프트",
    "광복절", "한글", "훈민정음", "김치", "비빔밥", "추석", "설날", "서울", "부산", "제주도", "독도",
    "고양이", "강아지", "펭귄", "고래", "상어", "코끼리", "기린", "판다", "개미", "벌",
    "산소", "이산화탄소", "물", "중력", "빛", "소리", "온도", "에너지", "전자", "양성자",
]


def rand_topic():
    r = random.random()
    if r < 0.6:
        return random.choice(TOPICS)
    if r < 0.85:
        return random.choice(NOUNS)
    return rand_word()


def obj(word):      # 을/를
    return "을" if has_batchim(word) else "를"


def topic_ra(word):  # 이라고/라고
    return "이라고" if has_batchim(word) else "라고"


# ───────────────────────── 템플릿 ─────────────────────────
# {APP}, {TIME}, {QUERY}, {MEMO} 자리에 슬롯 값이 들어간다.
# {OBJ} = 바로 앞 슬롯에 맞는 을/를, {RA} = 이라고/라고
OPEN_V = ["켜줘", "켜", "열어줘", "열어", "실행해줘", "실행해", "실행", "켜주세요", "실행시켜줘",
          "열어줄래", "켜봐", "좀 켜줘", "켜줄래?", "띄워줘", "켜 줘", "실행 좀", "열어 주세요", "켜라"]
CLOSE_V = ["꺼줘", "꺼", "닫아줘", "닫아", "종료해줘", "종료", "꺼주세요", "끄자", "좀 꺼줘",
           "닫아줄래", "종료시켜줘", "꺼 줘", "강제종료해", "끝내줘", "꺼라"]
TIMER_V = ["타이머 맞춰줘", "타이머", "뒤에 알려줘", "후에 알려줘", "뒤에 깨워줘", "알람 맞춰줘",
           "뒤에 불러줘", "타이머 설정해줘", "재줘", "뒤에 말해줘", "있다가 알려줘"]

INTENTS = {
    "open_app": [
        "{APP} " + v for v in OPEN_V
    ] + [
        "{APP}{OBJ} " + v for v in OPEN_V[:8]
    ] + [
        "{APP} 좀 " + v for v in ["켜줘", "열어줘", "실행해줘"]
    ] + ["{APP} 들어가줘", "{APP} 키자", "{APP} 열기", "{APP} 실행하기", "{APP} 켜줄 수 있어?"],

    "close_app": [
        "{APP} " + v for v in CLOSE_V
    ] + [
        "{APP}{OBJ} " + v for v in CLOSE_V[:8]
    ] + ["{APP} 그만 켜놔", "{APP} 닫기", "{APP} 끄기", "{APP} 꺼줄 수 있어?"],

    "set_timer": [
        "{TIME} " + v for v in TIMER_V
    ] + [
        "{TIME} 타이머", "타이머 {TIME}", "타이머 {TIME} 맞춰줘", "{TIME}만 재줘",
        "{TIME} 있다가 알려줘", "{TIME} 지나면 알려줘", "{TIME} 뒤에 라면 다 됐다고 알려줘",
        "{TIME} 후에 알람", "알람 {TIME} 뒤로", "{TIME} 카운트다운", "{NOUN} {TIME} 타이머", "{NOUN} {TIME} 타이머 맞춰줘",
        "{NOUN} {TIME} 재줘", "{NOUN} 하게 {TIME} 뒤에 알려줘", "{NOUN} 끓일 거니까 {TIME} 타이머",
    ],

    "ask_time": [
        "지금 몇 시야", "몇 시야", "몇시야?", "지금 몇시", "시간 알려줘", "현재 시간",
        "지금 시간 몇 시야", "몇 시 몇 분이야", "시간 좀 알려줘", "지금 몇 시예요?", "시간 몇시임",
    ],

    "ask_date": [
        "오늘 며칠이야", "오늘 날짜", "날짜 알려줘", "오늘 무슨 요일이야", "오늘 몇 월 며칠이야",
        "며칠이야?", "무슨 요일이야", "오늘 몇일이야", "몇일이지", "오늘 몇일", "날짜 몇일이야", "오늘 날짜 알려줘", "오늘이 며칠이지", "요일 알려줘",
    ],

    "web_search": [
        "{QUERY} 검색해줘", "{QUERY} 검색", "{QUERY} 찾아줘", "{QUERY} 찾아봐", "구글에 {QUERY} 검색해줘",
        "{QUERY} 검색 좀", "인터넷에서 {QUERY} 찾아줘", "{QUERY} 검색해봐", "{QUERY} 좀 찾아줘",
        "네이버에 {QUERY} 검색", "{QUERY}에 대해 찾아봐", "{QUERY} 구글링해줘", "검색해줘 {QUERY}",
        "{QUERY} 알아봐줘",
    ],

    "youtube": [
        "유튜브에서 {QUERY} 틀어줘", "유튜브 {QUERY}", "{QUERY} 틀어줘", "{QUERY} 유튜브로 틀어줘",
        "유튜브에 {QUERY} 검색해줘", "{QUERY} 보여줘", "유튜브로 {QUERY} 보여줘", "{QUERY} 재생해줘",
        "{QUERY} 틀어", "{QUERY} 좀 틀어줘", "유튜브 {QUERY} 찾아줘", "{QUERY} 듣고 싶어",
    ],

    "volume_up": [
        "소리 키워줘", "볼륨 올려줘", "소리 좀 크게", "볼륨 업", "소리 키워", "볼륨 높여줘",
        "너무 작아 소리 키워줘", "소리 더 크게 해줘", "볼륨 키워", "소리 올려",
    ],
    "volume_down": [
        "소리 줄여줘", "볼륨 내려줘", "소리 좀 작게", "볼륨 다운", "소리 줄여", "볼륨 낮춰줘",
        "너무 시끄러워 소리 줄여", "소리 더 작게 해줘", "볼륨 줄여", "소리 내려",
    ],
    "mute": [
        "음소거", "음소거 해줘", "소리 꺼줘", "조용히 해", "뮤트", "소리 없애줘", "음소거 풀어줘",
        "뮤트 해제", "음소거 켜", "소리 끄기",
    ],

    "screenshot": [
        "스크린샷 찍어줘", "캡처해줘", "화면 캡처", "스샷", "스크린샷", "화면 찍어줘", "캡쳐해줘",
        "스샷 좀", "화면 저장해줘", "지금 화면 캡처해",
    ],
    "lock": [
        "컴퓨터 잠가줘", "화면 잠금", "잠금", "pc 잠가", "컴퓨터 잠금", "잠가줘", "화면 잠가줘",
        "자리 비울게 잠가줘", "락 걸어줘",
    ],
    "shutdown": [
        "컴퓨터 꺼줘", "컴퓨터 종료", "pc 꺼", "시스템 종료", "컴 꺼줘", "컴퓨터 끄자", "전원 꺼줘",
        "pc 종료해줘", "컴퓨터 종료해", "이제 컴 끌게",
    ],

    "memo_add": [
        "{MEMO} 메모해줘", "{MEMO}{RA} 메모해줘", "메모 {MEMO}", "{MEMO} 적어줘", "{MEMO} 기억해줘",
        "{MEMO} 적어놔", "메모해줘 {MEMO}", "{MEMO}{RA} 적어놔", "{MEMO} 메모", "할 일 {MEMO}",
        "{MEMO} 할 일에 추가해줘", "{MEMO} 잊지 않게 메모",
    ],
    "memo_read": [
        "메모 보여줘", "메모 읽어줘", "할 일 뭐 있어", "내 메모", "메모 확인", "적어둔 거 알려줘",
        "할 일 목록", "메모 뭐 있었지", "할 일 알려줘", "메모 목록",
    ],
    "memo_clear": [
        "메모 다 지워줘", "메모 삭제", "메모 전부 삭제해줘", "할 일 다 지워", "메모 초기화",
        "메모 비워줘", "할 일 목록 지워줘",
    ],

    "greet": [
        "안녕", "안녕하세요", "하이", "ㅎㅇ", "반가워", "야", "헬로", "좋은 아침", "왔어", "나 왔어",
        "hello", "hi",
    ],
    "thanks": [
        "고마워", "고마워요", "땡큐", "ㄱㅅ", "ㄳ", "ㄳㄳ", "ㄱㅅㄱㅅ", "고맙다", "감사합니다", "수고했어", "잘했어", "오 굿", "덕분이야", "thx",
    ],
    "bye": [
        "잘 가", "바이", "ㅂㅇ", "나 간다", "종료해", "끝", "그만", "이제 됐어", "잘 자", "나중에 봐",
    ],
    "who_are_you": [
        "너 누구야", "넌 뭐야", "이름이 뭐야", "너 뭐 할 수 있어", "할 수 있는 거 알려줘", "도움말",
        "기능 알려줘", "뭐 할 줄 알아", "넌 뭘 할 수 있니", "뭘 할 수 있어", "어떤 기능 있어", "너 할 줄 아는 거 뭐야", "자기소개 해봐", "명령어 목록",
    ],

    # 지식 질문 → 지식 저장소/위키백과에서 찾아봄
    "ask_knowledge": [
        "{TOPIC}{IGA} 뭐야", "{TOPIC}{EUN} 뭐야", "{TOPIC} 뭐야", "{TOPIC}{IGA} 뭔데", "{TOPIC}{IGA} 뭐지",
        "{TOPIC}{EUN} 누구야", "{TOPIC} 누구야", "{TOPIC}{IGA} 누구지", "{TOPIC}에 대해 알려줘",
        "{TOPIC}에 대해 설명해줘", "{TOPIC} 설명해줘", "{TOPIC}{IGA} 뭔지 알려줘", "{TOPIC}{IGA} 무슨 뜻이야",
        "{TOPIC} 뜻", "{TOPIC} 뜻이 뭐야", "{TOPIC}에 대해 아는 거 있어?", "{TOPIC} 알아?",
        "{TOPIC}{EUN} 어떤 사람이야", "{TOPIC}{EUN} 어디야", "{TOPIC}{IGA} 어디야", "{TOPIC}에 대해 공부해줘",
        "{TOPIC}{IGA} 뭔지 궁금해", "{TOPIC}{EUN} 왜 생겨", "{TOPIC}{EUN} 어떻게 생겼어", "{TOPIC} 정보 알려줘",
        "{TOPIC}에 대해 찾아봐줘", "{TOPIC}{IGA} 뭐예요", "{TOPIC}{EUN} 무엇인가요",
    ],

    # 모르는 말(학습 안 한 잡담 등) → "잘 모르겠다"고 하게 만드는 클래스
    "unknown": [
        "오늘 너무 피곤하다", "배고프다", "롤 할까", "심심해", "이거 맞아?", "아 몰라", "ㅋㅋㅋㅋ",
        "그게 무슨 말이야", "사랑해", "인생이란 뭘까", "내일 학교 가기 싫다", "점심 뭐 먹지",
        "어제 꿈 이상했어", "너 바보야?", "흠", "음...", "그렇구나", "진짜?", "아니야", "응",
        "ㄴㄴ", "몰라", "재밌는 얘기 해줘", "노래 불러줘", "수학 문제 풀어줘", "1 더하기 1은",
        "지구는 둥글까", "고양이가 좋아 강아지가 좋아", "숙제 대신 해줘", "게임 추천해줘",
        "너 몇 살이야", "너 남자야 여자야", "몇 살이야", "{NOUN} 먹고 싶다", "{NOUN} 먹고 싶어",
        "{NOUN} 좋아해?", "{NOUN} 어때", "{NOUN} 싫어", "{NOUN} 했어", "오늘 {NOUN} 맛없었어",
        "{NOUN} 재밌었어", "{NOUN} 짜증나", "{NOUN} 사고 싶다", "{NOUN} 가고 싶다", "나 {NOUN} 좋아",
        "{NOUN} 때문에 힘들어", "{NOUN}{OBJ} 왜 하는 거야", "{NOUN} 진짜 별로다", "내일 {NOUN} 있어",
    ],
}


def fill(template):
    """템플릿 → (문장, 글자별 태그) 생성"""
    text, tags = "", []
    last_value = ""
    for part in re.split(r"(\{[A-Z]+\})", template):
        if not part:
            continue
        if part in ("{APP}", "{TIME}", "{QUERY}", "{MEMO}", "{NOUN}", "{TOPIC}"):
            slot = part[1:-1]
            if slot == "APP":
                val = random.choice(APPS) if random.random() < 0.8 else rand_word()
            elif slot == "TIME":
                val = rand_duration()
            elif slot == "QUERY":
                val = rand_query()
            elif slot == "MEMO":
                val = rand_memo()
            elif slot == "TOPIC":
                val = rand_topic()
            else:  # {NOUN}: unknown 문장용, 슬롯 아님
                val = random.choice(NOUNS)
                last_value = val
                text += val
                tags += ["O"] * len(val)
                continue
            last_value = val
            for i, ch in enumerate(val):
                text += ch
                tags.append(("B-" if i == 0 else "I-") + slot)
        elif part == "{OBJ}":
            j = obj(last_value)
            text += j
            tags.append("O")
        elif part in ("{IGA}", "{EUN}"):
            j = (("이" if has_batchim(last_value) else "가") if part == "{IGA}"
                 else ("은" if has_batchim(last_value) else "는"))
            text += j
            tags.append("O")
        elif part == "{RA}":
            j = topic_ra(last_value)
            text += j
            tags += ["O"] * len(j)
        else:
            text += part
            tags += ["O"] * len(part)
    return text, tags


PREFIXES = ["", "", "", "", "야 ", "저기 ", "혹시 ", "비서야 ", "어 ", "음 ", "빨리 ", "지금 "]
SUFFIXES = ["", "", "", "", "!", "?", "~", " ㅋㅋ", ".", " 좀", " 부탁해", " 빨리"]


def augment(text, tags):
    pre, suf = random.choice(PREFIXES), random.choice(SUFFIXES)
    text = pre + text + suf
    tags = ["O"] * len(pre) + tags + ["O"] * len(suf)
    # 띄어쓰기 랜덤 제거 (사람들이 대충 치는 것 흉내)
    if random.random() < 0.25:
        keep = [(c, t) for c, t in zip(text, tags) if c != " " or random.random() < 0.3]
        # 공백 다음 글자가 B 태그였으면 그대로 유지됨
        text = "".join(c for c, _ in keep)
        tags = [t for _, t in keep]
    return text, tags


def generate(n_per_intent=1200, seed=0):
    random.seed(seed)
    data = []
    for intent, templates in INTENTS.items():
        for _ in range(n_per_intent):
            t, tags = fill(random.choice(templates))
            t, tags = augment(t, tags)
            if t.strip():
                data.append((t, tags, intent))
    # 쓰레기 입력도 unknown으로
    junk = "가나다라마바사아자차카타파하ㅋㅎㅇㄴabcxyz123 "
    for _ in range(400):
        s = "".join(random.choice(junk) for _ in range(random.randint(1, 12)))
        data.append((s, ["O"] * len(s), "unknown"))
    for _ in range(400):  # 키보드 막 친 영어/한글
        s = rand_word() if random.random() < 0.5 else "".join(random.choice("qwertyuiopasdfghjklzxcvbnm") for _ in range(random.randint(2, 10)))
        data.append((s, ["O"] * len(s), "unknown"))
    # 대화 데이터의 사용자 말 → "명령 아님"(unknown) 예시로 추가 → 잡담은 대화 모델로 넘어감
    # (인사/감사/작별/자기소개 주제는 원래 의도가 따로 있어서 제외)
    import chat_data
    skip_words = ("안녕", "좋은 아침", "잘 자", "나 왔어", "고마워", "너 누구야", "미안", "응", "아니", "누가 만들었어")
    topics = [t for t in chat_data.TOPICS if not any(w in t[0] for w in skip_words)]
    vals = chat_data.SLOTS
    for _ in range(n_per_intent * 2):
        users, _r = random.choice(topics)
        u = chat_data.render(random.choice(users), {k: random.choice(v) for k, v in vals.items()})
        u = random.choice(chat_data.NOISE_PRE) + u + random.choice(chat_data.NOISE_SUF)
        data.append((u, ["O"] * len(u), "unknown"))
    random.shuffle(data)
    return data


if __name__ == "__main__":
    for t, tags, it in generate(3)[:20]:
        print(it, "|", t)

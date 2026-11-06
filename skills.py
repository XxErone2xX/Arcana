"""
비서가 실제로 하는 일들 (윈도우 기준)
새 기능 추가 방법:
  1) data.py의 INTENTS에 새 의도 + 문장 템플릿 추가
  2) 여기에 함수 만들고 맨 아래 SKILLS에 등록
  3) python train.py 로 다시 학습
"""
import os
import re
import sys
import json
import time
import ctypes
import difflib
import datetime
import threading
import subprocess
import webbrowser
import urllib.parse

IS_WIN = sys.platform == "win32"
DRY_RUN = os.environ.get("ASSISTANT_DRY_RUN") == "1"   # 테스트용: 실제로 실행 안 함
BASE = os.path.dirname(os.path.abspath(__file__))
MEMO_FILE = os.path.join(BASE, "memos.json")
SHOT_DIR = os.path.join(BASE, "screenshots")

# ───────────────────────── 앱 목록 ─────────────────────────
# 이름: (실행 명령, 종료할 프로세스 이름들)
# 실행 명령은 "start" 로 실행됨 → exe 이름, 경로, 프로토콜(discord:// 등) 다 가능
LOCAL = os.environ.get("LOCALAPPDATA", "")
APPS = {
    "크롬":        ("chrome", ["chrome.exe"]),
    "엣지":        ("msedge", ["msedge.exe"]),
    "메모장":      ("notepad", ["notepad.exe"]),
    "계산기":      ("calc", ["CalculatorApp.exe", "Calculator.exe"]),
    "디스코드":    ("discord://", ["Discord.exe"]),
    "로블록스":    ("https://www.roblox.com/home", ["RobloxPlayerBeta.exe"]),
    "로블록스스튜디오": ("roblox-studio:", ["RobloxStudioBeta.exe"]),
    "탐색기":      ("explorer", []),       # 탐색기는 끄면 작업표시줄까지 꺼져서 종료 막아둠
    "스포티파이":  ("spotify:", ["Spotify.exe"]),
    "그림판":      ("mspaint", ["mspaint.exe"]),
    "작업관리자":  ("taskmgr", ["Taskmgr.exe"]),
    "vscode":      ("code", ["Code.exe"]),
    "카카오톡":    (r'"C:\Program Files (x86)\Kakao\KakaoTalk\KakaoTalk.exe"', ["KakaoTalk.exe"]),
    "스팀":        ("steam://open/main", ["steam.exe"]),
    "설정":        ("ms-settings:", ["SystemSettings.exe"]),
    "명령프롬프트": ("cmd", []),
    "파워쉘":      ("powershell", []),
    "블렌더":      ("blender", ["blender.exe"]),
    "오토핫키":    ("autohotkey", ["AutoHotkey.exe", "AutoHotkeyU64.exe", "AutoHotkeyU32.exe"]),
}
# 다른 이름으로 불러도 알아듣게
ALIASES = {
    "chrome": "크롬", "인터넷": "크롬", "구글": "크롬", "edge": "엣지",
    "디코": "디스코드", "discord": "디스코드", "롤블록스": "로블록스", "roblox": "로블록스",
    "로블록스스튜디오": "로블록스스튜디오", "스튜디오": "로블록스스튜디오",
    "파일탐색기": "탐색기", "파일": "탐색기", "spotify": "스포티파이",
    "비주얼스튜디오코드": "vscode", "vs코드": "vscode", "코드": "vscode",
    "카톡": "카카오톡", "steam": "스팀", "cmd": "명령프롬프트", "powershell": "파워쉘",
    "blender": "블렌더", "ahk": "오토핫키",
}


def find_app(name):
    if not name:
        return None
    n = re.sub(r"\s+", "", name.lower())
    n = re.sub(r"(을|를|좀)$", "", n)
    if n in APPS:
        return n
    if n in ALIASES:
        return ALIASES[n]
    keys = list(APPS) + list(ALIASES)
    m = difflib.get_close_matches(n, keys, n=1, cutoff=0.5)
    if m:
        return ALIASES.get(m[0], m[0])
    return None


def run(cmd):
    if DRY_RUN:
        return
    subprocess.Popen(f'start "" {cmd}', shell=True)


# ───────────────────────── 기능들 ─────────────────────────
def open_app(slots, ctx):
    key = find_app(slots.get("APP"))
    if not key:
        return f"'{slots.get('APP', '?')}'은(는) 내가 모르는 프로그램이야. skills.py의 APPS에 추가해줘."
    run(APPS[key][0])
    return f"{key} 켤게."


def close_app(slots, ctx):
    key = find_app(slots.get("APP"))
    if not key:
        return f"'{slots.get('APP', '?')}'은(는) 내가 모르는 프로그램이야."
    procs = APPS[key][1]
    if not procs:
        return f"{key}는 끄면 위험해서 안 끌게."
    if not DRY_RUN and IS_WIN:
        for p in procs:
            subprocess.run(["taskkill", "/IM", p, "/F"], capture_output=True)
    return f"{key} 껐어."


KOR_NUM = {"일": 1, "이": 2, "삼": 3, "사": 4, "오": 5, "육": 6, "칠": 7, "팔": 8, "구": 9,
           "한": 1, "두": 2, "세": 3, "네": 4, "다섯": 5, "여섯": 6, "일곱": 7, "여덟": 8, "아홉": 9, "열": 10}


def kor_to_int(s):
    if s.isdigit():
        return int(s)
    if s in KOR_NUM:
        return KOR_NUM[s]
    # 이십오 → 25
    total = 0
    if "십" in s:
        a, b = s.split("십", 1)
        total = (KOR_NUM.get(a, 1) if a else 1) * 10
        s = b
    return total + (KOR_NUM.get(s, 0) if s else 0)


def parse_duration(text):
    if not text:
        return None
    sec = 0
    for num, unit in re.findall(r"([0-9]+|[일이삼사오육칠팔구십한두세네다섯여섯일곱여덟아홉열]+)\s*(시간|분|초)", text):
        n = kor_to_int(num)
        sec += n * {"시간": 3600, "분": 60, "초": 1}[unit]
    return sec or None


def fmt_duration(sec):
    h, r = divmod(sec, 3600)
    m, s = divmod(r, 60)
    parts = [f"{h}시간" if h else "", f"{m}분" if m else "", f"{s}초" if s else ""]
    return " ".join(p for p in parts if p)


def alert(msg):
    print(f"\n⏰ {msg}\n> ", end="", flush=True)
    if IS_WIN and not DRY_RUN:
        try:
            import winsound
            for _ in range(3):
                winsound.Beep(880, 300)
            ctypes.windll.user32.MessageBoxW(0, msg, "Arcana", 0x40000 | 0x40)  # 맨 위에 표시
        except Exception:
            pass


def set_timer(slots, ctx):
    sec = parse_duration(slots.get("TIME", "")) or parse_duration(ctx["text"])
    if not sec:
        return "몇 분 뒤에 알려줄지 못 알아들었어. '5분 타이머'처럼 말해줘."
    label = fmt_duration(sec)
    t = threading.Timer(sec, alert, args=[f"{label} 지났어!"])
    t.daemon = True
    t.start()
    return f"{label} 뒤에 알려줄게."


WEEK = ["월", "화", "수", "목", "금", "토", "일"]


def ask_time(slots, ctx):
    n = datetime.datetime.now()
    ampm = "오전" if n.hour < 12 else "오후"
    h = n.hour % 12 or 12
    return f"지금 {ampm} {h}시 {n.minute}분이야."


def ask_date(slots, ctx):
    n = datetime.datetime.now()
    return f"오늘은 {n.year}년 {n.month}월 {n.day}일 {WEEK[n.weekday()]}요일이야."


def web_search(slots, ctx):
    q = slots.get("QUERY") or ctx["text"]
    if not DRY_RUN:
        webbrowser.open("https://www.google.com/search?q=" + urllib.parse.quote(q))
    return f"'{q}' 검색할게."


def youtube(slots, ctx):
    q = slots.get("QUERY") or ctx["text"]
    if not DRY_RUN:
        webbrowser.open("https://www.youtube.com/results?search_query=" + urllib.parse.quote(q))
    return f"유튜브에서 '{q}' 찾았어."


def press_key(vk, times=1):
    if not IS_WIN or DRY_RUN:
        return
    for _ in range(times):
        ctypes.windll.user32.keybd_event(vk, 0, 0, 0)
        ctypes.windll.user32.keybd_event(vk, 0, 2, 0)


def volume_up(slots, ctx):
    press_key(0xAF, 5)
    return "소리 키웠어."


def volume_down(slots, ctx):
    press_key(0xAE, 5)
    return "소리 줄였어."


def mute(slots, ctx):
    press_key(0xAD)
    return "음소거 전환했어."


def screenshot(slots, ctx):
    os.makedirs(SHOT_DIR, exist_ok=True)
    path = os.path.join(SHOT_DIR, datetime.datetime.now().strftime("%Y%m%d_%H%M%S.png"))
    if DRY_RUN:
        return f"캡처했어 → {path}"
    try:
        from PIL import ImageGrab
        ImageGrab.grab(all_screens=True).save(path)
        return f"캡처했어 → {path}"
    except ImportError:
        # Pillow 없으면 윈도우 기본 단축키(Win + PrtSc) 사용
        u = ctypes.windll.user32
        u.keybd_event(0x5B, 0, 0, 0); u.keybd_event(0x2C, 0, 0, 0)
        u.keybd_event(0x2C, 0, 2, 0); u.keybd_event(0x5B, 0, 2, 0)
        return "캡처했어 (사진 > 스크린샷 폴더에 저장됨)."


def lock(slots, ctx):
    if IS_WIN and not DRY_RUN:
        ctypes.windll.user32.LockWorkStation()
    return "화면 잠갔어."


def shutdown(slots, ctx):
    ans = ctx["ask"]("진짜 컴퓨터 끌까? (응/아니) ")
    if ans.strip() not in ("응", "ㅇㅇ", "어", "네", "예", "y", "yes", "ㅇ"):
        return "안 끌게."
    if IS_WIN and not DRY_RUN:
        subprocess.run(["shutdown", "/s", "/t", "10"])
    return "10초 뒤에 꺼져. 취소하려면 cmd에 shutdown /a 입력해."


def _load_memos():
    try:
        with open(MEMO_FILE, encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return []


def _save_memos(m):
    with open(MEMO_FILE, "w", encoding="utf-8") as f:
        json.dump(m, f, ensure_ascii=False, indent=2)


def memo_add(slots, ctx):
    memo = slots.get("MEMO")
    if not memo:
        return "뭘 메모할지 못 알아들었어."
    m = _load_memos()
    m.append({"text": memo, "time": datetime.datetime.now().strftime("%m/%d %H:%M")})
    _save_memos(m)
    return f"'{memo}' 메모했어."


def memo_read(slots, ctx):
    m = _load_memos()
    if not m:
        return "메모 하나도 없어."
    return "메모 목록:\n" + "\n".join(f"  {i+1}. {x['text']} ({x['time']})" for i, x in enumerate(m))


def memo_clear(slots, ctx):
    _save_memos([])
    return "메모 다 지웠어."


def greet(slots, ctx):
    return "어 왔어? 뭐 해줄까?"


def thanks(slots, ctx):
    return "별말을 다 ㅋㅋ"


def bye(slots, ctx):
    ctx["quit"] = True
    return "ㅂㅇ 나중에 또 불러."


def who_are_you(slots, ctx):
    return ("나는 Arcana, 네 PC 비서야. 할 수 있는 거:\n"
            "  프로그램 켜기/끄기, 타이머, 시간·날짜, 구글 검색, 유튜브 틀기,\n"
            "  소리 키우기/줄이기/음소거, 스크린샷, 화면 잠금, 컴퓨터 끄기, 메모 쓰기/보기/지우기")


def unknown(slots, ctx):
    return "그건 아직 못 배웠어. ('뭐 할 수 있어?' 라고 물어보면 기능 알려줄게)"


SKILLS = {name: fn for name, fn in globals().items()
          if callable(fn) and name in (
              "open_app close_app set_timer ask_time ask_date web_search youtube volume_up "
              "volume_down mute screenshot lock shutdown memo_add memo_read memo_clear "
              "greet thanks bye who_are_you unknown").split()}

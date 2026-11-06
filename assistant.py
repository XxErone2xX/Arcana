"""
비서 실행:  python assistant.py
  -v 옵션을 붙이면 AI가 뭐라고 판단했는지(의도, 확신도, 슬롯) 같이 보여줌
"""
import os
import sys
from model import load_bundle, predict
from skills import SKILLS

THRESHOLD = 0.55   # 이 확신도보다 낮으면 "모르겠다"로 처리
BASE = os.path.dirname(os.path.abspath(__file__))


def main():
    verbose = "-v" in sys.argv
    model, tok, intents, tags = load_bundle(os.path.join(BASE, "assistant_model.pt"))
    print("Arcana 준비됐어. 할 말 입력해 (끄려면 '바이')\n")
    ctx = {"quit": False, "ask": input}
    while not ctx["quit"]:
        try:
            text = input("> ").strip()
        except (EOFError, KeyboardInterrupt):
            break
        if not text:
            continue
        intent, conf, slots = predict(model, tok, intents, tags, text)
        if verbose:
            print(f"   [의도: {intent} {conf:.0%}  슬롯: {slots}]")
        if conf < THRESHOLD:
            intent = "unknown"
        ctx["text"] = text
        try:
            print(SKILLS[intent](slots, ctx))
        except Exception as e:
            print(f"하다가 오류 났어: {e}")


if __name__ == "__main__":
    main()

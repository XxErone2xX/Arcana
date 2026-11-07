// 입력창 + 한 글자씩 써지는 대답 (대화 탭, 학습 탭, 빠른 호출 창이 같이 씀)

// ── 입력창: Enter 보내기 · Shift+Enter 줄바꿈 · 내용에 맞춰 높이가 늘어남 ──
export function textbox(el, onSubmit, { maxHeight = 168 } = {}) {
  const fit = () => {
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, maxHeight) + "px";
    el.style.overflowY = el.scrollHeight > maxHeight ? "auto" : "hidden";
  };
  el.addEventListener("input", fit);
  el.addEventListener("keydown", (e) => {
    // 한글 조합 중 Enter 는 글자 확정용이라 무시
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing && e.keyCode !== 229) {
      e.preventDefault();
      onSubmit(el.value);
    }
  });
  fit();
  return { fit, clear: () => { el.value = ""; fit(); } };
}

// ── 한 글자씩 써지기 ──
// 긴 대답도 너무 오래 걸리지 않게 속도를 맞춤 (짧으면 천천히, 길면 빠르게, 최대 약 2.5초)
export function typeOut(el, text, { onTick, render } = {}) {
  return new Promise((resolve) => {
    const chars = [...text];
    const perFrame = Math.max(1, Math.ceil(chars.length / 150));
    const node = document.createTextNode("");
    const caret = Object.assign(document.createElement("span"), { className: "caret" });
    el.append(node, caret);
    let i = 0;
    let skip = false;
    const finish = () => {
      caret.remove();
      node.remove();
      if (render) render(el, text);
      else el.append(document.createTextNode(text));
      onTick?.();
      resolve();
    };
    el.addEventListener("click", () => (skip = true), { once: true }); // 누르면 바로 다 보여주기
    const step = () => {
      if (skip) return finish();
      i = Math.min(chars.length, i + perFrame);
      node.data = chars.slice(0, i).join("");
      onTick?.();
      if (i >= chars.length) return finish();
      // 문장 끝에서 아주 잠깐 쉼
      const last = chars[i - 1];
      setTimeout(() => requestAnimationFrame(step), /[.?!。\n]/.test(last) && chars.length < 300 ? 90 : 16);
    };
    requestAnimationFrame(step);
  });
}

// ── 맨 아래를 보고 있을 때만 자동으로 따라 내려가기 ──
export function stickyScroll(box) {
  let stick = true;
  box.addEventListener("scroll", () => {
    stick = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
  }, { passive: true });
  return {
    follow: () => { if (stick) box.scrollTop = box.scrollHeight; },
    force: () => { stick = true; box.scrollTop = box.scrollHeight; },
  };
}

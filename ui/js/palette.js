// "/" 를 입력하면 입력창 위에 명령어 목록이 뜨는 기능 (위/아래 화살표, Enter·Tab으로 선택, Esc로 닫기)
//
// 사용법:
//   attachPalette(inputElement, [{ name: "/Start", desc: "설명", icon: ICONS.play, run?: () => {} }, ...], onPick)
//   - run 이 있으면 고르는 즉시 실행, 없으면 입력창에 "/이름 " 을 채우고 onPick(command) 호출

const svg = (d) => `<svg viewBox="0 0 24 24"><path d="${d}"/></svg>`;
export const ICONS = {
  play: svg("M8 5v14l11-7z"),
  stop: svg("M7 7h10v10H7z"),
  chart: svg("M5 19V11M12 19V5M19 19v-6"),
  skip: svg("M6 6l7 6-7 6M15 6v12"),
  teach: svg("M4 7l8-4 8 4-8 4-8-4zM7 9v5c2 2 8 2 10 0V9"),
  help: svg("M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6v.6M12 17h.01M12 21a9 9 0 1 1 0-18 9 9 0 0 1 0 18z"),
  clear: svg("M5 7h14M10 11v6M14 11v6M6 7l1 12h10l1-12M9 7V4h6v3"),
  book: svg("M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3V4zM5 17a3 3 0 0 1 3-3h11"),
  gear: svg("M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"),
  chat: svg("M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12z"),
};

export function attachPalette(input, commands, onPick) {
  const host = input.closest("form");
  const box = document.createElement("div");
  box.className = "palette";
  box.hidden = true;
  host.appendChild(box);

  let items = [];
  let active = 0;

  const close = () => { box.hidden = true; items = []; };

  function render() {
    const v = input.value;
    // "/" 로 시작하고 아직 띄어쓰기 전일 때만 (명령 이름 입력 중)
    if (!v.startsWith("/") || v.includes(" ")) return close();
    const q = v.slice(1).toLowerCase();
    items = commands.filter((c) => c.name.slice(1).toLowerCase().startsWith(q) || (c.alias ?? []).some((a) => a.startsWith(q)));
    if (!items.length) return close();
    active = Math.min(active, items.length - 1);
    box.replaceChildren(
      ...items.map((c, i) => {
        const row = document.createElement("button");
        row.type = "button";
        row.className = "palette-item" + (i === active ? " active" : "");
        row.innerHTML = `<span class="p-icon">${c.icon ?? ""}</span>`;
        row.append(
          Object.assign(document.createElement("span"), { className: "p-name", textContent: c.name }),
          Object.assign(document.createElement("span"), { className: "p-desc", textContent: c.desc }),
        );
        row.onmouseenter = () => { active = i; highlight(); };
        row.onmousedown = (e) => { e.preventDefault(); pick(i); };
        return row;
      }),
    );
    box.hidden = false;
  }

  function highlight() {
    [...box.children].forEach((el, i) => el.classList.toggle("active", i === active));
    box.children[active]?.scrollIntoView({ block: "nearest" });
  }

  function pick(i) {
    const c = items[i];
    if (!c) return;
    close();
    if (c.run) {
      input.value = "";
      input.dispatchEvent(new Event("input"));
      c.run();
      return;
    }
    input.value = c.name + (c.needsArgs ? " " : "");
    input.dispatchEvent(new Event("input"));
    input.focus();
    onPick?.(c);
  }

  input.addEventListener("input", () => { active = 0; render(); });
  input.addEventListener("focus", render);
  input.addEventListener("blur", () => setTimeout(close, 100));
  input.addEventListener(
    "keydown",
    (e) => {
      if (box.hidden) return;
      if (e.key === "ArrowDown") { active = (active + 1) % items.length; highlight(); }
      else if (e.key === "ArrowUp") { active = (active - 1 + items.length) % items.length; highlight(); }
      else if (e.key === "Enter" || e.key === "Tab") { pick(active); }
      else if (e.key === "Escape") { close(); }
      else return;
      e.preventDefault();
      e.stopImmediatePropagation();
    },
    true, // 다른 키 처리(전송, 이전 명령)보다 먼저
  );
}

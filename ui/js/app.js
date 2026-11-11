import { api, ready } from "./api.js";
import { attachPalette, ICONS } from "./palette.js";
import { textbox, typeOut, stickyScroll } from "./compose.js";
import { initSettings, openSettings } from "./settings.js";

const $ = (s) => document.querySelector(s);
const log = $("#log");
const input = $("#input");
const sendBtn = $("#send");

let info = null;
const scroller = stickyScroll(log);
const history = [];
let histPos = 0;

// 첫 화면 추천 명령
const SUGGESTIONS = ["뭐 할 수 있어?", "라면 3분 타이머", "디코 켜줘", "오늘 너무 피곤해"];

// ───────────── 대화 표시 ─────────────
function addMsg(who, text, opts = {}) {
  $("#welcome")?.remove();
  const el = document.createElement("div");
  el.className = `msg ${who}` + (opts.error ? " error" : "") + (opts.alert ? " alert" : "");
  const body = document.createElement("div");
  body.className = "body";
  const content = document.createElement("div");
  content.className = "content";
  body.appendChild(content);
  el.appendChild(body);
  log.appendChild(el);
  const extras = () => {
    if (opts.debug) {
      const d = document.createElement("span");
      d.className = "debug";
      d.textContent = opts.debug;
      body.appendChild(d);
    }
  };
  if (opts.type) {
    // 대답은 한 글자씩
    body.done = typeOut(content, text, { render: linkify, onTick: scroller.follow }).then(extras);
  } else {
    linkify(content, text);
    extras();
    body.done = Promise.resolve();
  }
  if (who === "user") scroller.force(); else scroller.follow();
  return body;
}

// 대답 속 웹 주소를 누를 수 있는 링크로 (기본 브라우저로 열림)
function linkify(el, text) {
  const re = /https?:\/\/[^\s<>"'）)]+/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    el.appendChild(document.createTextNode(text.slice(last, m.index)));
    const a = document.createElement("a");
    a.href = "#";
    a.className = "link";
    a.textContent = m[0];
    a.onclick = (e) => { e.preventDefault(); api.openUrl(m[0]); };
    el.appendChild(a);
    last = m.index + m[0].length;
  }
  el.appendChild(document.createTextNode(text.slice(last)));
}

function typing() {
  const el = document.createElement("div");
  el.className = "msg bot";
  el.innerHTML = '<div class="typing"><span></span><span></span><span></span><em class="think-note"></em></div>';
  log.appendChild(el);
  scroller.follow();
  return el;
}

function debugLine(r) {
  const u = r.understanding;
  const slots = Object.entries(u.slots).map(([k, v]) => `${k}=${v}`).join(", ");
  return `${u.intent} · ${(u.confidence * 100).toFixed(0)}%${r.fallback ? " (낮아서 무시)" : ""}${slots ? " · " + slots : ""}`;
}

function button(text, cls) {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = text;
  b.className = cls;
  return b;
}

async function showReply(reply, r, userText) {
  const body = addMsg("bot", reply.text, { type: true, error: reply.error, debug: info?.show_debug && r ? debugLine(r) : null });
  await body.done;
  if (reply.confirm && r) {
    const box = document.createElement("div");
    box.className = "actions";
    const yes = button("예", "btn primary");
    const no = button("아니요", "btn");
    yes.onclick = async () => { box.remove(); showReply(await api.confirm(r.understanding.intent)); };
    no.onclick = () => { box.remove(); addMsg("bot", "알겠습니다. 취소했습니다."); };
    box.append(yes, no);
    body.appendChild(box);
  }
  if (reply.from_chat && userText) body.appendChild(feedbackBar(userText, reply.text));
  if (reply.via) body.appendChild(Object.assign(document.createElement("div"), { className: "via", textContent: reply.via }));
  scroller.follow();
  if (reply.quit) setTimeout(() => api.window.close(), 900);
}

// ───────────── 👍/👎 (3단계: 피드백 학습) ─────────────
const ICON_UP = '<svg viewBox="0 0 24 24"><path d="M7 11v9H4v-9h3zm0 0l4-8c1.5 0 2.5 1 2.5 2.5V9h5a2 2 0 0 1 2 2.3l-1.2 7A2 2 0 0 1 17.3 20H7"/></svg>';
const ICON_DOWN = '<svg viewBox="0 0 24 24"><path d="M17 13V4h3v9h-3zm0 0l-4 8c-1.5 0-2.5-1-2.5-2.5V15h-5a2 2 0 0 1-2-2.3l1.2-7A2 2 0 0 1 6.7 4H17"/></svg>';

function feedbackBar(userText, replyText) {
  const bar = document.createElement("div");
  bar.className = "fb";
  const up = button("", "fb-btn");
  const down = button("", "fb-btn");
  up.innerHTML = ICON_UP;
  down.innerHTML = ICON_DOWN;
  up.title = "좋은 대답";
  down.title = "별로인 대답";
  const done = (btn) => { up.disabled = down.disabled = true; btn.classList.add("chosen"); };
  up.onclick = async () => { done(up); toast(await api.feedback(userText, replyText, 1, null)); };
  down.onclick = () => {
    done(down);
    const form = document.createElement("form");
    form.className = "fb-better";
    const inp = Object.assign(document.createElement("input"), { placeholder: "더 좋은 대답을 알려 주세요 (비워도 됨)", maxLength: 200 });
    const ok = button("보내기", "btn primary");
    ok.type = "submit";
    form.append(inp, ok);
    form.onsubmit = async (e) => {
      e.preventDefault();
      form.remove();
      toast(await api.feedback(userText, replyText, -1, inp.value.trim() || null));
    };
    bar.after(form);
    inp.focus();
  };
  bar.append(up, down);
  return bar;
}

// ───────────── 보내기 ─────────────
async function submit(text) {
  text = text.trim();
  if (!text) return;
  history.push(text);
  histPos = history.length;
  addMsg("user", text);
  box.clear();
  sendBtn.disabled = true;
  const t = typing();
  try {
    const r = await api.send(text);
    t.remove();
    showReply(r.reply, r, text);
  } catch (e) {
    t.remove();
    addMsg("bot", "오류가 발생했습니다. " + e, { error: true });
  }
}

const box = textbox(input, (v) => submit(v));
$("#composer").addEventListener("submit", (e) => { e.preventDefault(); submit(input.value); });
input.addEventListener("input", () => { sendBtn.disabled = !input.value.trim(); });
input.addEventListener("keydown", (e) => {
  // 이전 명령: 한 줄일 때만 (여러 줄이면 화살표는 줄 이동)
  const oneLine = !input.value.includes("\n");
  if (e.key === "ArrowUp" && oneLine && histPos > 0) { input.value = history[--histPos]; box.fit(); e.preventDefault(); }
  else if (e.key === "ArrowDown" && oneLine && histPos < history.length) { histPos++; input.value = history[histPos] ?? ""; box.fit(); e.preventDefault(); }
  else if (e.key === "Escape") box.clear();
  sendBtn.disabled = !input.value.trim();
});

// 대화 탭 "/" 명령어 목록
attachPalette(input, [
  { name: "/Learn", desc: "학습 탭으로 가기", icon: ICONS.book, alias: ["학습"], run: () => document.querySelector('.tab[data-view="study"]').click() },
  { name: "/Clear", desc: "대화 내용 지우기", icon: ICONS.clear, alias: ["지우기"], run: () => { log.replaceChildren(); addMsg("bot", "대화 내용을 지웠습니다."); } },
  { name: "/Settings", desc: "설정 열기", icon: ICONS.gear, alias: ["설정"], run: () => openSettings("settings") },
  { name: "/Model", desc: "모델·생각 수치", icon: ICONS.chart, alias: ["모델", "생각"], run: () => openSettings("model") },
  { name: "/Memory", desc: "메모리 보기", icon: ICONS.book, alias: ["메모리", "기억"], run: () => openSettings("settings", "memory") },
  { name: "/Help", desc: "할 수 있는 일 보기", icon: ICONS.help, alias: ["도움말"], run: () => submit("뭐 할 수 있어?") },
]);

for (const s of SUGGESTIONS) {
  const b = button(s, "");
  b.onclick = () => submit(s);
  $("#suggest").appendChild(b);
}

// ───────────── 알림 (타이머 등) ─────────────
function chime() {
  try {
    const ctx = new AudioContext();
    [0, 0.18].forEach((t, i) => {
      const o = ctx.createOscillator(), g = ctx.createGain();
      o.type = "sine";
      o.frequency.value = i ? 1046 : 784;
      o.connect(g); g.connect(ctx.destination);
      g.gain.setValueAtTime(0.0001, ctx.currentTime + t);
      g.gain.exponentialRampToValueAtTime(0.12, ctx.currentTime + t + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + t + 0.6);
      o.start(ctx.currentTime + t); o.stop(ctx.currentTime + t + 0.6);
    });
  } catch {}
}

function toast(text, title) {
  const t = document.createElement("div");
  t.className = "toast";
  if (title) t.appendChild(Object.assign(document.createElement("b"), { textContent: title }));
  t.appendChild(document.createTextNode(text));
  $("#toasts").appendChild(t);
  setTimeout(() => t.remove(), title ? 6000 : 2500);
}

api.onEvent((e) => {
  if (e.kind === "alert") {
    chime();
    addMsg("bot", e.body, { alert: true });
    toast(e.body, e.title);
  } else if (e.kind === "say") {
    addMsg("bot", e.text, { type: true });
  } else if (e.kind === "thinking") {
    const n = log.querySelector(".typing .think-note");
    if (n) n.textContent = e.text;
  }
});

// ───────────── 메뉴·설정 (settings.js) ─────────────
$("#btn-menu").onclick = () => openSettings();
initSettings({ getInfo: () => info, setInfo: (i) => (info = i), toast, checkUpdate: () => checkUpdate(true) });

// ───────────── 창 버튼 ─────────────
$("#btn-min").onclick = () => api.window.minimize();
$("#btn-close").onclick = () => api.window.close();

// ───────────── 업데이트 ─────────────
async function checkUpdate(manual) {
  let u;
  try {
    u = await api.checkUpdate();
  } catch (e) {
    if (manual) toast(String(e));
    return;
  }
  if (!u.available) { if (manual) toast(`최신 버전입니다 (v${u.current})`); return; }
  if (!manual && !info.auto_update) {
    addMsg("bot", `새 버전 v${u.latest}이 나왔습니다. 메뉴 → 업데이트 확인을 누르시면 받을 수 있습니다.`);
    return;
  }
  $("#overlay-text").textContent = `v${u.latest}로 업데이트 중…`;
  $("#overlay").hidden = false;
  try {
    await api.applyUpdate(); // 성공하면 앱이 새 버전으로 다시 켜짐
  } catch (e) {
    $("#overlay").hidden = true;
    addMsg("bot", "업데이트하지 못했습니다. " + e, { error: true });
  }
}

// 켤 때 자동 업데이트 (Rust 쪽에서 바로 확인 → 새 버전이면 받아서 다시 켬)
api.onUpdating((v) => {
  $("#overlay-text").textContent = `v${v}로 업데이트 중…`;
  $("#overlay").hidden = false;
});
api.onUpdateFailed((e) => {
  $("#overlay").hidden = true;
  addMsg("bot", "업데이트하지 못했습니다. " + e, { error: true });
});

// ───────────── 시작 ─────────────
(async () => {
  info = await ready(); // 앱이 다 켜질 때까지 기다림 (안 그러면 설정 화면이 기본값으로 보임)
  $("#version").textContent = "v" + info.version;
  document.title = info.app_name;
  $("#app-name").textContent = info.app_name;
  input.placeholder = `${info.name}에게 말하기`;
  if (info.hotkey_enabled && info.hotkey) $("#hint").textContent = `Enter 보내기 · Shift+Enter 줄바꿈 · 어디서든 ${info.hotkey}로 부르기`;
  input.focus();
})();


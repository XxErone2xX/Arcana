import { api } from "./api.js";

const $ = (s) => document.querySelector(s);
const log = $("#log");
const input = $("#input");
const sendBtn = $("#send");

let info = null;
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
  body.textContent = text;
  if (opts.debug) {
    const d = document.createElement("span");
    d.className = "debug";
    d.textContent = opts.debug;
    body.appendChild(d);
  }
  el.appendChild(body);
  log.appendChild(el);
  log.scrollTop = log.scrollHeight;
  return body;
}

function typing() {
  const el = document.createElement("div");
  el.className = "msg bot";
  el.innerHTML = '<div class="typing"><span></span><span></span><span></span></div>';
  log.appendChild(el);
  log.scrollTop = log.scrollHeight;
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

function showReply(reply, r, userText) {
  const body = addMsg("bot", reply.text, { error: reply.error, debug: info?.show_debug && r ? debugLine(r) : null });
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
  input.value = "";
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

$("#composer").addEventListener("submit", (e) => { e.preventDefault(); submit(input.value); });
input.addEventListener("input", () => { sendBtn.disabled = !input.value.trim(); });
input.addEventListener("keydown", (e) => {
  if (e.key === "ArrowUp" && histPos > 0) { input.value = history[--histPos]; e.preventDefault(); }
  if (e.key === "ArrowDown") { histPos = Math.min(history.length, histPos + 1); input.value = history[histPos] ?? ""; }
  if (e.key === "Escape") input.value = "";
  sendBtn.disabled = !input.value.trim();
});

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
    addMsg("bot", e.text);
  }
});

// ───────────── 메뉴 ─────────────
const drawer = $("#drawer");
$("#btn-menu").onclick = () => { drawer.hidden = !drawer.hidden; if (!drawer.hidden) renderDrawer(); };
drawer.addEventListener("click", (e) => { if (e.target === drawer) drawer.hidden = true; });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") drawer.hidden = true; });

function renderDrawer() {
  $("#caps").replaceChildren(...info.capabilities.map((c) => Object.assign(document.createElement("li"), { textContent: c })));
  $("#opt-debug").checked = info.show_debug;
  $("#opt-update").checked = info.auto_update;
  const rows = [["버전", "v" + info.version], ["명령 모델", info.brain], ["대화 모델", info.talker ?? "없음"], ["데이터", info.data_dir]];
  if (info.missing_skills.length) rows.push(["스킬 없음", info.missing_skills.join(", ")]);
  $("#about").replaceChildren(...rows.flatMap(([k, v]) => [
    Object.assign(document.createElement("dt"), { textContent: k }),
    Object.assign(document.createElement("dd"), { textContent: v }),
  ]));
}

$("#opt-debug").onchange = (e) => { info.show_debug = e.target.checked; api.setOption("show_debug", e.target.checked); };
$("#opt-update").onchange = (e) => { info.auto_update = e.target.checked; api.setOption("auto_update", e.target.checked); };
$("#btn-config").onclick = () => api.openConfig();
$("#btn-reload").onclick = async () => { await api.reloadConfig(); info = await api.info(); renderDrawer(); toast("설정을 다시 불러왔습니다"); };
$("#btn-update").onclick = () => checkUpdate(true);

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

// ───────────── 시작 ─────────────
(async () => {
  info = await api.info();
  $("#version").textContent = "v" + info.version;
  document.title = info.app_name;
  $("#app-name").textContent = info.app_name;
  input.placeholder = `${info.name}에게 명령하기`;
  input.focus();
  checkUpdate(false);
})();

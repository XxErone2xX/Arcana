import { api } from "./api.js";

const $ = (s) => document.querySelector(s);
const log = $("#log");
const input = $("#input");
const sendBtn = $("#send");

let info = null;
const history = [];
let histPos = 0;

// 첫 화면 추천 명령
const SUGGESTIONS = ["뭐 할 수 있어?", "라면 3분 타이머", "디코 켜줘", "유튜브에서 로파이 틀어줘"];

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

function showReply(reply, r) {
  const body = addMsg("bot", reply.text, { error: reply.error, debug: info?.show_debug && r ? debugLine(r) : null });
  if (reply.confirm && r) {
    const box = document.createElement("div");
    box.className = "actions";
    const yes = button("예", "btn primary");
    const no = button("아니", "btn");
    yes.onclick = async () => { box.remove(); showReply(await api.confirm(r.understanding.intent)); };
    no.onclick = () => { box.remove(); addMsg("bot", "알겠어, 안 할게."); };
    box.append(yes, no);
    body.appendChild(box);
  }
  if (reply.quit) setTimeout(() => api.window.close(), 900);
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
    showReply(r.reply, r);
  } catch (e) {
    t.remove();
    addMsg("bot", "오류: " + e, { error: true });
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
  const rows = [["버전", "v" + info.version], ["두뇌", info.brain], ["데이터", info.data_dir]];
  if (info.missing_skills.length) rows.push(["스킬 없음", info.missing_skills.join(", ")]);
  $("#about").replaceChildren(...rows.flatMap(([k, v]) => [
    Object.assign(document.createElement("dt"), { textContent: k }),
    Object.assign(document.createElement("dd"), { textContent: v }),
  ]));
}

$("#opt-debug").onchange = (e) => { info.show_debug = e.target.checked; api.setOption("show_debug", e.target.checked); };
$("#opt-update").onchange = (e) => { info.auto_update = e.target.checked; api.setOption("auto_update", e.target.checked); };
$("#btn-config").onclick = () => api.openConfig();
$("#btn-reload").onclick = async () => { await api.reloadConfig(); info = await api.info(); renderDrawer(); toast("설정 다시 읽었어"); };
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
  if (!u.available) { if (manual) toast(`최신 버전이야 (v${u.current})`); return; }
  if (!manual && !info.auto_update) {
    addMsg("bot", `새 버전 v${u.latest} 나왔어. 메뉴 → 업데이트 확인을 누르면 받을 수 있어.`);
    return;
  }
  $("#overlay-text").textContent = `v${u.latest}로 업데이트 중…`;
  $("#overlay").hidden = false;
  try {
    await api.applyUpdate(); // 성공하면 앱이 새 버전으로 다시 켜짐
  } catch (e) {
    $("#overlay").hidden = true;
    addMsg("bot", "업데이트 실패: " + e, { error: true });
  }
}

// ───────────── 시작 ─────────────
(async () => {
  info = await api.info();
  $("#version").textContent = "v" + info.version;
  document.title = info.name;
  input.placeholder = `${info.name}에게 명령하기`;
  input.focus();
  checkUpdate(false);
})();

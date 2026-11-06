import { api } from "./api.js";

const $ = (s) => document.querySelector(s);
const log = $("#log");
const input = $("#input");

let info = null;
const history = [];
let histPos = 0;

// 입력창 아래 추천 명령
const SUGGESTIONS = ["뭐 할 수 있어?", "디코 켜줘", "라면 3분 타이머", "지금 몇 시야", "유튜브에서 로파이 틀어줘", "메모 보여줘"];

// ───────────── 대화 표시 ─────────────
function addMsg(who, text, opts = {}) {
  const el = document.createElement("div");
  el.className = `msg ${who}` + (who === "bot" ? " corners" : "") + (opts.error ? " error" : "") + (opts.alert ? " alert" : "");
  el.textContent = text;
  if (opts.debug) {
    const d = document.createElement("span");
    d.className = "debug";
    d.textContent = opts.debug;
    el.appendChild(d);
  }
  log.appendChild(el);
  log.scrollTop = log.scrollHeight;
  return el;
}

function typing() {
  const el = document.createElement("div");
  el.className = "msg bot corners typing";
  el.innerHTML = "<span></span><span></span><span></span>";
  log.appendChild(el);
  log.scrollTop = log.scrollHeight;
  return el;
}

function debugLine(r) {
  const u = r.understanding;
  const slots = Object.entries(u.slots).map(([k, v]) => `${k}=${v}`).join(", ");
  return `의도 ${u.intent} · 확신 ${(u.confidence * 100).toFixed(0)}%${r.fallback ? " (낮아서 무시)" : ""}${slots ? " · " + slots : ""}`;
}

function showReply(reply, r) {
  const el = addMsg("bot", reply.text, { error: reply.error, debug: info?.show_debug && r ? debugLine(r) : null });
  if (reply.confirm && r) {
    const box = document.createElement("div");
    box.className = "actions";
    const yes = button("예", "btn primary corners");
    const no = button("아니", "btn dim corners");
    yes.onclick = async () => { box.remove(); showReply(await api.confirm(r.understanding.intent)); };
    no.onclick = () => { box.remove(); addMsg("bot", "알겠어, 안 할게.", {}).classList.add("corners"); };
    box.append(yes, no);
    el.appendChild(box);
  }
  if (reply.quit) setTimeout(() => api.window.close(), 900);
}

function button(text, cls) {
  const b = document.createElement("button");
  b.textContent = text;
  b.className = cls;
  return b;
}

// ───────────── 보내기 ─────────────
async function submit(text) {
  text = text.trim();
  if (!text) return;
  history.push(text);
  histPos = history.length;
  addMsg("user", text);
  input.value = "";
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
input.addEventListener("keydown", (e) => {
  if (e.key === "ArrowUp" && histPos > 0) { input.value = history[--histPos]; e.preventDefault(); }
  if (e.key === "ArrowDown") { histPos = Math.min(history.length, histPos + 1); input.value = history[histPos] ?? ""; }
  if (e.key === "Escape") input.value = "";
});

for (const s of SUGGESTIONS) {
  const c = button(s, "chip");
  c.onclick = () => submit(s);
  $("#chips").appendChild(c);
}

// ───────────── 알림 (타이머 등) ─────────────
function beep() {
  try {
    const ctx = new AudioContext();
    [0, 0.25, 0.5].forEach((t) => {
      const o = ctx.createOscillator(), g = ctx.createGain();
      o.frequency.value = 880; o.connect(g); g.connect(ctx.destination);
      g.gain.setValueAtTime(0.15, ctx.currentTime + t);
      g.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + t + 0.2);
      o.start(ctx.currentTime + t); o.stop(ctx.currentTime + t + 0.2);
    });
  } catch {}
}

api.onEvent((e) => {
  if (e.kind === "alert") {
    beep();
    addMsg("bot", `⏰ ${e.body}`, { alert: true });
    const t = document.createElement("div");
    t.className = "toast corners";
    t.innerHTML = `<b></b><span></span>`;
    t.querySelector("b").textContent = e.title;
    t.querySelector("span").textContent = e.body;
    $("#toasts").appendChild(t);
    setTimeout(() => t.remove(), 6000);
  } else if (e.kind === "say") {
    addMsg("bot", e.text);
  }
});

// ───────────── 메뉴 ─────────────
const drawer = $("#drawer");
$("#btn-menu").onclick = () => { drawer.hidden = !drawer.hidden; if (!drawer.hidden) renderDrawer(); };
drawer.addEventListener("click", (e) => { if (e.target === drawer) drawer.hidden = true; });

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
$("#btn-reload").onclick = async () => { await api.reloadConfig(); info = await api.info(); renderDrawer(); toastText("설정 다시 읽었어"); };
$("#btn-update").onclick = () => checkUpdate(true);

function toastText(text) {
  const t = document.createElement("div");
  t.className = "toast corners";
  t.textContent = text;
  $("#toasts").appendChild(t);
  setTimeout(() => t.remove(), 2500);
}

// ───────────── 창 버튼 ─────────────
$("#btn-min").onclick = () => api.window.minimize();
$("#btn-close").onclick = () => api.window.close();

// ───────────── 업데이트 ─────────────
async function checkUpdate(manual) {
  try {
    const u = await api.checkUpdate();
    if (!u.available) { if (manual) toastText(`최신 버전이야 (v${u.current})`); return; }
    if (!manual && !info.auto_update) {
      addMsg("bot", `새 버전 v${u.latest} 나왔어. 메뉴 → 업데이트 확인 눌러서 받을 수 있어.`);
      return;
    }
    $("#overlay-text").textContent = `v${u.current} → v${u.latest} 업데이트 중…`;
    $("#overlay").hidden = false;
    await api.applyUpdate(); // 성공하면 앱이 새 버전으로 다시 켜짐
  } catch (e) {
    $("#overlay").hidden = true;
    if (manual) toastText(String(e));
  }
}

// ───────────── 시작 ─────────────
(async () => {
  info = await api.info();
  $("#version").textContent = "v" + info.version;
  document.title = info.name;
  addMsg("bot", `${info.name} 준비 완료. 뭐 해줄까?`);
  input.focus();
  checkUpdate(false);
})();

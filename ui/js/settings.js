// 설정 창: 설정(일반·빠른 호출·메모리·컨테이너 메모리) / 모델(생각 수치) / 정보
import { api, ready } from "./api.js";

const $ = (s) => document.querySelector(s);
const sheet = $("#sheet");
let ctx = null; // { getInfo, setInfo, toast, checkUpdate }

// 생각 수치 1~5
const THINK = {
  1: ["빠르게", "대답을 2개 만들어 보고 고릅니다. 가장 빠릅니다."],
  2: ["가볍게", "대답을 4개 만들어 보고 고릅니다."],
  3: ["보통", "대답을 8개 만들어 보고 가장 좋은 것을 고릅니다. (기본)"],
  4: ["깊게", "대답을 16개 이상 만들고, 한 번 더 신중하게 다시 생각합니다."],
  5: ["PrimeThink", "스스로 채점해서 이 대답이 완벽하다고 판단할 때까지 시간 제한 없이 계속 다시 생각합니다. 생각할수록 더 신중해집니다."],
};

// 컨테이너 메모리 크기: 슬라이더 0~100 → 10MB ~ 10GB (로그 눈금)
const toMb = (p) => Math.round(10 * Math.pow(1024, p / 100));
const toPos = (mb) => Math.round((100 * Math.log(mb / 10)) / Math.log(1024));
const niceMb = (mb) => (mb >= 1024 ? `${(mb / 1024).toFixed(mb >= 10240 ? 0 : 1)} GB` : `${mb} MB`);
const snap = (mb) => {
  for (const s of [10, 25, 50, 100, 250, 500, 1024, 2048, 5120, 10240]) if (Math.abs(mb - s) / s < 0.12) return s;
  return mb >= 1024 ? Math.round(mb / 256) * 256 : mb >= 100 ? Math.round(mb / 10) * 10 : mb;
};
const bytes = (b) => (b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(2)} GB` : b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(1)} MB` : `${Math.max(0, Math.round(b / 1024))} KB`);

const KIND = { name: "이름", birthday: "생일", like: "좋아함", dislike: "싫어함", note: "메모" };

export function openSettings(page = "settings", section) {
  sheet.hidden = false;
  showPage(page);
  render();
  if (section) setTimeout(() => $(`#sec-${section}`)?.scrollIntoView({ behavior: "smooth", block: "start" }), 30);
}

function close() { sheet.hidden = true; }

function showPage(page) {
  for (const t of document.querySelectorAll("#sheet-tabs .tab")) t.classList.toggle("active", t.dataset.page === page);
  for (const p of document.querySelectorAll(".sheet .page")) p.hidden = p.dataset.page !== page;
  $(".sheet-body").scrollTop = 0;
}

function rows(el, list) {
  el.replaceChildren(...list.flatMap(([k, v]) => [
    Object.assign(document.createElement("dt"), { textContent: k }),
    Object.assign(document.createElement("dd"), { textContent: v }),
  ]));
}

async function render() {
  // 아직 정보를 못 받았으면 받아 온 뒤에 그림 (기본값이 보이는 문제 방지)
  if (!ctx.getInfo()) ctx.setInfo(await ready());
  const info = ctx.getInfo();
  $("#opt-debug").checked = info.show_debug;
  $("#opt-update").checked = info.auto_update;
  $("#opt-hotkey").checked = info.hotkey_enabled;
  $("#hotkey-btn").textContent = info.hotkey;
  $("#hotkey-error").hidden = !info.hotkey_error;
  $("#hotkey-error").textContent = info.hotkey_error ?? "";
  $("#opt-memory").checked = info.memory_enabled;
  $("#opt-container").checked = info.container_enabled;
  $("#think-range").value = info.think_level;
  $("#opt-autotrain").checked = info.auto_train;
  $("#opt-robots").checked = info.obey_robots;
  const idle = $("#idle-range");
  idle.value = info.auto_train_idle_min;
  idle.style.setProperty("--p", ((info.auto_train_idle_min - 1) / 59) * 100 + "%");
  $("#idle-label").textContent = `${info.auto_train_idle_min}분`;
  $("#autotrain-status").textContent = info.auto_train_status ?? "";
  renderThink(info.think_level);
  $("#caps").replaceChildren(...info.capabilities.map((c) => Object.assign(document.createElement("li"), { textContent: c })));
  rows($("#model-info"), [
    ["모델", "Arcana (처음부터 직접 만든 모델)"],
    ["명령 이해", info.brain],
    ["대화", info.talker ?? "아직 없음 — 학습 탭의 /Train 으로 만들 수 있습니다"],
  ]);
  const r = [["버전", "v" + info.version], ["데이터", info.data_dir]];
  if (info.missing_skills.length) r.push(["스킬 없음", info.missing_skills.join(", ")]);
  rows($("#about"), r);
  renderFacts();
  renderContainer();
}

function renderThink(level) {
  const [title, desc] = THINK[level];
  $("#think-title").textContent = level == 5 ? "PrimeThink 활성화" : `${level}단계 · ${title}`;
  $("#think-desc").textContent = desc;
  $("#think").classList.toggle("prime", level == 5);
  const r = $("#think-range");
  r.style.setProperty("--p", ((level - 1) / 4) * 100 + "%");
}

async function renderFacts() {
  const facts = await api.memoryList();
  const ul = $("#facts");
  if (!facts.length) {
    ul.replaceChildren(Object.assign(document.createElement("li"), { className: "empty", textContent: "아직 기억하고 있는 것이 없습니다." }));
    return;
  }
  ul.replaceChildren(...facts.map((f) => {
    const li = document.createElement("li");
    li.append(
      Object.assign(document.createElement("span"), { className: "kind", textContent: KIND[f.kind] ?? f.kind }),
      Object.assign(document.createElement("span"), { className: "val", textContent: f.value }),
    );
    const del = Object.assign(document.createElement("button"), { className: "icon-btn small", title: "지우기" });
    del.innerHTML = '<svg viewBox="0 0 24 24"><path d="M7 7l10 10M17 7L7 17"/></svg>';
    del.onclick = async () => { await api.memoryRemove(f.id); renderFacts(); };
    li.appendChild(del);
    return li;
  }));
}

async function renderContainer() {
  const c = await api.containerInfo();
  const r = $("#container-range");
  if (document.activeElement !== r) r.value = toPos(c.max_mb);
  $("#container-size").textContent = niceMb(c.max_mb);
  const pct = Math.min(100, (c.used_bytes / (c.max_mb * 1024 * 1024)) * 100);
  $("#container-bar").style.width = Math.max(pct, c.used_bytes ? 0.8 : 0) + "%";
  $("#container-used").textContent = `${bytes(c.used_bytes)} 사용 중 · 최근 ${c.turns}개 대화로 맥락 파악`;
  r.style.setProperty("--p", r.value + "%");
}

// 단축키 바꾸기: 버튼 누르고 원하는 키 조합 누르기
function captureHotkey(btn) {
  const before = btn.textContent;
  btn.textContent = "키를 누르세요… (Esc 취소)";
  btn.classList.add("listening");
  const onKey = async (e) => {
    e.preventDefault();
    if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
    window.removeEventListener("keydown", onKey, true);
    btn.classList.remove("listening");
    if (e.key === "Escape") { btn.textContent = before; return; }
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
    let key = e.code.replace(/^Key/, "").replace(/^Digit/, "");
    if (key === "Space") key = "Space";
    if (!mods.length && !/^F\d+$/.test(key)) { ctx.toast("Ctrl, Alt, Shift 중 하나와 같이 눌러 주세요"); btn.textContent = before; return; }
    const combo = [...mods, key].join("+");
    try {
      await api.setValue("hotkey", combo);
      btn.textContent = combo;
      ctx.toast(`빠른 호출 단축키: ${combo}`);
    } catch (err) {
      btn.textContent = before;
      ctx.toast(String(err));
    }
    await refresh();
  };
  window.addEventListener("keydown", onKey, true);
}

async function refresh() {
  ctx.setInfo(await api.info());
  render();
}

export function initSettings(c) {
  ctx = c;
  $("#sheet-close").onclick = close;
  sheet.addEventListener("click", (e) => { if (e.target === sheet) close(); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !sheet.hidden && !$("#hotkey-btn").classList.contains("listening")) close(); });
  for (const t of document.querySelectorAll("#sheet-tabs .tab")) t.onclick = () => showPage(t.dataset.page);

  const bool = (id, key, opt) => {
    $(id).onchange = async (e) => {
      try {
        if (opt) await api.setOption(key, e.target.checked);
        else await api.setValue(key, e.target.checked);
      } catch (err) { ctx.toast(String(err)); }
      refresh();
    };
  };
  bool("#opt-debug", "show_debug", true);
  bool("#opt-update", "auto_update", true);
  bool("#opt-hotkey", "hotkey_enabled");
  bool("#opt-memory", "memory_enabled");
  bool("#opt-container", "container_enabled");
  bool("#opt-autotrain", "auto_train");
  bool("#opt-robots", "obey_robots");
  const idle = $("#idle-range");
  idle.oninput = () => {
    idle.style.setProperty("--p", ((idle.value - 1) / 59) * 100 + "%");
    $("#idle-label").textContent = `${idle.value}분`;
  };
  idle.onchange = async () => { await api.setValue("auto_train_idle_min", +idle.value); refresh(); };

  $("#hotkey-btn").onclick = (e) => captureHotkey(e.currentTarget);
  $("#btn-config").onclick = () => api.openConfig();
  $("#btn-reload").onclick = async () => { await api.reloadConfig(); await refresh(); ctx.toast("설정을 다시 불러왔습니다"); };
  $("#btn-update").onclick = () => ctx.checkUpdate();

  $("#add-fact").onsubmit = async (e) => {
    e.preventDefault();
    const v = $("#fact-input").value.trim();
    if (!v) return;
    await api.memoryAdd(v);
    $("#fact-input").value = "";
    renderFacts();
  };
  $("#memory-clear").onclick = async () => {
    if (!confirmTwice($("#memory-clear"))) return;
    await api.memoryClear();
    renderFacts();
    ctx.toast("메모리를 모두 지웠습니다");
  };
  $("#container-clear").onclick = async () => {
    if (!confirmTwice($("#container-clear"))) return;
    await api.containerClear();
    renderContainer();
    ctx.toast("대화 기록을 지웠습니다");
  };

  const range = $("#container-range");
  range.oninput = () => {
    range.style.setProperty("--p", range.value + "%");
    $("#container-size").textContent = niceMb(snap(toMb(+range.value)));
  };
  range.onchange = async () => { await api.setValue("container_mb", snap(toMb(+range.value))); refresh(); };

  const think = $("#think-range");
  think.oninput = () => renderThink(+think.value);
  think.onchange = async () => {
    await api.setValue("think_level", +think.value);
    if (+think.value === 5) ctx.toast("PrimeThink가 활성화되었습니다");
    refresh();
  };
}

// 지우기 버튼은 한 번 더 눌러야 실행 (실수 방지)
function confirmTwice(btn) {
  if (btn.dataset.armed) { delete btn.dataset.armed; btn.textContent = btn.dataset.label; return true; }
  btn.dataset.label = btn.textContent;
  btn.dataset.armed = "1";
  btn.textContent = "한 번 더 누르면 지웁니다";
  setTimeout(() => { if (btn.dataset.armed) { delete btn.dataset.armed; btn.textContent = btn.dataset.label; } }, 3000);
  return false;
}

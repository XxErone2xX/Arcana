// 학습 탭: /Start 하면 Arcana가 스스로 공부하고, 모르는 게 생기면 질문함
import { api, ready } from "./api.js";
import { attachPalette, ICONS } from "./palette.js";
import { textbox, stickyScroll } from "./compose.js";

const $ = (s) => document.querySelector(s);
const feed = $("#feed");
const input = $("#study-input");
const sendBtn = $("#study-send");
let onStudyTab = false;
const scroller = stickyScroll(feed);

// ───────────── 탭 전환 ─────────────
document.querySelectorAll(".titlebar .tab").forEach((t) => {
  t.onclick = () => {
    document.querySelectorAll(".titlebar .tab").forEach((x) => x.classList.toggle("active", x === t));
    onStudyTab = t.dataset.view === "study";
    $("#view-chat").hidden = onStudyTab;
    $("#view-study").hidden = !onStudyTab;
    if (onStudyTab) {
      $("#study-dot").hidden = true;
      refresh();
      input.focus();
    } else {
      $("#input").focus();
    }
  };
});

// ───────────── 표시 ─────────────
function time() {
  const d = new Date();
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

// 받침에 맞는 을/를
function eul(w) {
  const c = w.trim().slice(-1).charCodeAt(0);
  if (c >= 0xac00 && c <= 0xd7a3) return (c - 0xac00) % 28 ? "을" : "를";
  return "을(를)";
}

function line(text, cls = "") {
  $("#feed-empty")?.remove();
  const el = document.createElement("div");
  el.className = "line " + cls;
  const t = document.createElement("time");
  t.textContent = time();
  const s = document.createElement("span");
  if (typeof text === "string") s.textContent = text;
  else s.append(...text);
  el.append(t, s);
  feed.appendChild(el);
  while (feed.children.length > 300) feed.firstChild.remove();
  if (cls === "me") scroller.force(); else scroller.follow();
}

function render(r) {
  for (const l of r.lines) line(l);
  const st = $("#study-state");
  st.textContent = r.running ? "학습 중" : "멈춤";
  st.classList.toggle("on", r.running);
  const s = r.stats;
  $("#study-stats").textContent = `알고 있는 문서 ${s.count}개 · 배운 대답 ${s.taught}개` + (s.questions ? ` · 질문 ${s.questions}개` : "");
  showQuestion(r.question);
}

function showQuestion(q) {
  $("#question").hidden = !q;
  if (q) $("#q-text").textContent = q;
}

async function refresh() {
  try { await ready(); render(await api.studyStatus()); } catch {}
}

// ───────────── 입력 ─────────────
async function submit(text) {
  text = text.trim();
  if (!text) return;
  line(text, "me");
  box.clear();
  sendBtn.disabled = true;
  try {
    render(await api.study(text));
  } catch (e) {
    line("오류가 발생했습니다. " + e);
  }
}

const box = textbox(input, (v) => submit(v));
$("#study-form").addEventListener("submit", (e) => { e.preventDefault(); submit(input.value); });
input.addEventListener("input", () => { sendBtn.disabled = !input.value.trim(); });
$("#q-skip").onclick = () => submit("/Skip");

// "/" 명령어 목록
attachPalette(input, [
  { name: "/Start", desc: "스스로 학습 시작 — /Start (링크) 는 그 링크부터", icon: ICONS.play, alias: ["시작"], run: () => submit("/Start") },
  { name: "/Start only", desc: "이 사이트에서만 공부 — /Start only (링크)", icon: ICONS.play, alias: ["만", "only"], needsArgs: true },
  { name: "/Stop", desc: "학습 멈추기", icon: ICONS.stop, alias: ["중지"], run: () => submit("/Stop") },
  { name: "/Status", desc: "지금까지 배운 것", icon: ICONS.chart, alias: ["상태"], run: () => submit("/Status") },
  { name: "/Skip", desc: "지금 질문 넘기기", icon: ICONS.skip, alias: ["넘기기"], run: () => submit("/Skip") },
  { name: "/Teach", desc: "대답 가르치기 — /Teach 말 = 대답", icon: ICONS.teach, alias: ["가르치기"], needsArgs: true },
  { name: "/Train", desc: "공부한 문서로 대화 모델 학습", icon: ICONS.book, alias: ["학습"], run: () => submit("/Train") },
  { name: "/Reload", desc: "새로 학습한 대화 모델 적용", icon: ICONS.play, alias: ["새로고침"], run: () => submit("/Reload") },
  { name: "/Diag", desc: "인터넷 연결 진단", icon: ICONS.chart, alias: ["진단"], run: () => submit("/Diag") },
  { name: "/Help", desc: "명령어 설명", icon: ICONS.help, alias: ["도움말"], run: () => submit("/Help") },
]);

// ───────────── 백그라운드 학습 알림 ─────────────
api.onEvent((e) => {
  if (e.kind === "learned") {
    const b = document.createElement("b");
    b.textContent = e.title;
    const site = Object.assign(document.createElement("span"), { className: "site", textContent: e.site });
    line([b, `${eul(e.title)} 공부했습니다. (알고 있는 문서 ${e.count}개) `, site], "learned");
    refresh();
  } else if (e.kind === "study_log") {
    line(e.text);
  } else if (e.kind === "question") {
    line("새 질문이 있습니다.", "ask");
    refresh();
    if (!onStudyTab) $("#study-dot").hidden = false;
  }
});

refresh();

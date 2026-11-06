// 학습 탭: /Start 하면 Arcana가 스스로 공부하고, 모르는 게 생기면 질문함
import { api } from "./api.js";

const $ = (s) => document.querySelector(s);
const feed = $("#feed");
const input = $("#study-input");
const sendBtn = $("#study-send");
let onStudyTab = false;

// ───────────── 탭 전환 ─────────────
document.querySelectorAll(".tab").forEach((t) => {
  t.onclick = () => {
    document.querySelectorAll(".tab").forEach((x) => x.classList.toggle("active", x === t));
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
  feed.scrollTop = feed.scrollHeight;
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
  try { render(await api.studyStatus()); } catch {}
}

// ───────────── 입력 ─────────────
async function submit(text) {
  text = text.trim();
  if (!text) return;
  line(text, "me");
  input.value = "";
  sendBtn.disabled = true;
  try {
    render(await api.study(text));
  } catch (e) {
    line("오류가 발생했습니다. " + e);
  }
}

$("#study-form").addEventListener("submit", (e) => { e.preventDefault(); submit(input.value); });
input.addEventListener("input", () => { sendBtn.disabled = !input.value.trim(); });
$("#q-skip").onclick = () => submit("/Skip");

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

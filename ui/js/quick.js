// 빠른 호출 창 (기본 Alt+Shift+A)
// 위 = Arcana (파랑·보라 물결), 아래 = 사용자.
//  - 열면 위아래가 같이 물결침
//  - 사용자가 입력하는 동안: 위는 검정, 아래가 빛남
//  - Arcana가 생각·대답하는 동안: 아래는 검정, 위가 빛남
//  - Esc (또는 빈 곳 클릭) = 닫기
import { api } from "./api.js";
import { textbox, typeOut } from "./compose.js";

const $ = (s) => document.querySelector(s);
const q = $("#q");
const reply = $("#reply");
const status = $("#status");
const sent = $("#sent");

// ───────────── 물결 ─────────────
const TOP = ["#1d3bff", "#4338ca", "#7c3aed", "#a855f7", "#38bdf8"]; // 파랑·남색·보라 (신비로운 쪽)
const BOTTOM = ["#6d28d9", "#4f46e5", "#2563eb", "#0ea5e9", "#c026d3"];

const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const lerp = (a, b, t) => a + (b - a) * t;

class Wave {
  constructor(canvas, side, palette) {
    this.c = canvas;
    this.x = canvas.getContext("2d");
    this.side = side;
    this.pal = palette.map(hex);
    this.mix = 1; // 1 = 색, 0 = 검정
    this.target = 1;
    this.grow = 0; // 0 = 숨김, 1 = 다 펼쳐짐
    this.growTarget = 0;
    this.layers = [
      { base: 0.62, amp: 0.10, freq: 1.1, speed: 0.42, phase: 0.0, alpha: 0.55 },
      { base: 0.48, amp: 0.12, freq: 1.7, speed: -0.6, phase: 1.7, alpha: 0.55 },
      { base: 0.36, amp: 0.09, freq: 2.4, speed: 0.78, phase: 3.1, alpha: 0.6 },
      { base: 0.24, amp: 0.07, freq: 3.1, speed: -0.95, phase: 4.4, alpha: 0.7 },
      { base: 0.13, amp: 0.05, freq: 4.2, speed: 1.15, phase: 0.9, alpha: 0.85 },
    ];
  }
  resize() {
    const s = 0.5 * (window.devicePixelRatio || 1); // 절반 해상도 → 가볍고 자연스럽게 번짐
    this.c.width = Math.max(1, Math.round(this.c.clientWidth * s));
    this.c.height = Math.max(1, Math.round(this.c.clientHeight * s));
  }
  color(i, a) {
    const [r, g, b] = this.pal[i % this.pal.length];
    const m = this.mix;
    // 검정일 때도 살짝 남색 기운이 남아서 화면이 "꺼진" 느낌은 안 나게
    // 검정(비활성)일 때는 연하게
    return `rgba(${lerp(2, r, m) | 0},${lerp(2, g, m) | 0},${lerp(8, b, m) | 0},${a * lerp(0.5, 1, m)})`;
  }
  draw(t) {
    const { x, c } = this;
    const w = c.width, h = c.height;
    this.mix += (this.target - this.mix) * 0.06; // 색·위치가 부드럽게 바뀜
    this.energy += (this.energyTarget - this.energy) * 0.04;
    this.grow += (this.growTarget - this.grow) * 0.09;
    x.clearRect(0, 0, w, h);
    const g = this.grow;
    if (g < 0.002) return;
    const top = this.side === "top";
    const steps = 72;
    // 비활성(검정)일 때는 물결 층이 화면 가장자리 쪽으로 내려가 얇아짐, 활성이면 원래 위치
    const sink = lerp(0.58, 1, this.mix);
    const edgeY = (L, px) => {
      const u = px / w;
      // 천천히 숨 쉬듯 높이·세기가 바뀌는 물결 (여러 사인파를 겹쳐서 반복이 티 나지 않게)
      const breathe = 1 + 0.07 * Math.sin(t * 0.21 + L.phase * 2) + 0.04 * Math.sin(t * 0.53 + L.phase);
      const amp = L.amp * h * g * sink * (1 + 0.3 * this.energy) * (1 + 0.28 * Math.sin(t * 0.37 + L.phase));
      const y = L.base * h * g * sink * breathe
        + amp * Math.sin(u * Math.PI * 2 * L.freq + t * L.speed + L.phase)
        + amp * 0.45 * Math.sin(u * Math.PI * 2 * L.freq * 0.47 - t * L.speed * 0.63 + L.phase * 1.3)
        + amp * 0.18 * Math.sin(u * Math.PI * 2 * L.freq * 2.3 + t * L.speed * 1.7);
      return top ? y : h - y;
    };
    this.layers.forEach((L, i) => {
      x.beginPath();
      x.moveTo(0, top ? 0 : h);
      for (let k = 0; k <= steps; k++) {
        const px = (k / steps) * w;
        x.lineTo(px, edgeY(L, px));
      }
      x.lineTo(w, top ? 0 : h);
      x.closePath();
      const reach = (L.base + L.amp * 1.6) * h * g * sink;
      const grad = x.createLinearGradient(0, top ? 0 : h, 0, top ? reach : h - reach);
      grad.addColorStop(0, this.color(i, L.alpha));
      grad.addColorStop(0.65, this.color(i + 1, L.alpha * 0.55));
      grad.addColorStop(1, this.color(i + 2, 0));
      x.fillStyle = grad;
      x.fill();
    });
    // 맨 앞 물결 가장자리에 빛나는 선 (활성일 때만)
    if (this.mix > 0.05) {
      const L = this.layers[1];
      x.beginPath();
      for (let k = 0; k <= steps; k++) {
        const px = (k / steps) * w;
        k ? x.lineTo(px, edgeY(L, px)) : x.moveTo(px, edgeY(L, px));
      }
      x.strokeStyle = `rgba(200,190,255,${0.35 * this.mix * g})`;
      x.lineWidth = 1.5;
      x.stroke();
    }
  }
}

const top = new Wave($("#wave-top"), "top", TOP);
const bottom = new Wave($("#wave-bottom"), "bottom", BOTTOM);
top.energy = bottom.energy = top.energyTarget = bottom.energyTarget = 0;
let running = false;

function frame(ms) {
  const t = ms / 1000;
  top.draw(t);
  bottom.draw(t);
  const visible = top.grow > 0.002 || bottom.grow > 0.002 || top.growTarget || bottom.growTarget;
  if (visible) requestAnimationFrame(frame);
  else running = false;
}
function animate() {
  if (!running) { running = true; requestAnimationFrame(frame); }
}
function resize() { top.resize(); bottom.resize(); }
window.addEventListener("resize", resize);

// ───────────── 상태 ─────────────
// open: 둘 다 빛남 · typing: 위 검정 · answering: 아래 검정
function mode(m) {
  document.body.classList.toggle("answering", m === "answering");
  top.target = m === "typing" ? 0 : 1;
  bottom.target = m === "answering" ? 0 : 1;
  top.energyTarget = m === "answering" ? 1 : 0;
  bottom.energyTarget = m === "typing" ? 0.4 : 0;
}

let token = 0; // 닫았다 열면 이전 대답은 무시
let pendingConfirm = null;
const box = textbox(q, (v) => submit(v), { maxHeight: 220 });

function open() {
  token++;
  pendingConfirm = null;
  reply.textContent = "";
  status.textContent = "";
  sent.textContent = "";
  box.clear();
  resize();
  document.body.classList.remove("closed");
  top.mix = bottom.mix = 1;
  top.growTarget = bottom.growTarget = 1;
  mode("open");
  animate();
  setTimeout(() => q.focus(), 30);
}

function close() {
  token++;
  document.body.classList.add("closed");
  top.growTarget = bottom.growTarget = 0;
  q.blur();
  setTimeout(() => { if (document.body.classList.contains("closed")) api.quickHide(); }, 380);
}

function linkify(el, text) {
  const re = /https?:\/\/[^\s<>"'）)]+/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    el.appendChild(document.createTextNode(text.slice(last, m.index)));
    const a = Object.assign(document.createElement("a"), { textContent: m[0] });
    a.onclick = (e) => { e.preventDefault(); api.openUrl(m[0]); };
    el.appendChild(a);
    last = m.index + m[0].length;
  }
  el.appendChild(document.createTextNode(text.slice(last)));
}

function thinking(text) {
  status.innerHTML = '<span class="dots"><span></span><span></span><span></span></span> ';
  status.append(document.createTextNode(text ?? "생각 중"));
}

async function say(text, mine) {
  reply.textContent = "";
  status.textContent = "";
  await typeOut(reply, text, { render: linkify, onTick: () => (reply.scrollTop = reply.scrollHeight) });
  if (mine !== token) return;
}

async function submit(raw) {
  const text = raw.trim();
  const mine = token;
  // "진행할까요?" 질문 뒤에 그냥 Enter = 예
  if (pendingConfirm && !text) {
    const intent = pendingConfirm;
    pendingConfirm = null;
    mode("answering");
    thinking();
    const r = await api.confirm(intent);
    if (mine === token) await say(r.text, mine);
    return;
  }
  if (!text) return;
  pendingConfirm = null;
  sent.textContent = text;
  box.clear();
  mode("answering");
  reply.textContent = "";
  thinking();
  try {
    const r = await api.send(text);
    if (mine !== token) return; // 그새 닫았음
    await say(r.reply.text, mine);
    if (mine !== token) return;
    if (r.reply.confirm) {
      pendingConfirm = r.understanding.intent;
      status.textContent = "Enter를 누르면 진행합니다";
    }
    if (r.reply.quit) setTimeout(close, 900);
  } catch (e) {
    if (mine === token) await say("오류가 발생했습니다. " + e, mine);
  }
}

// 입력하기 시작하면 위(Arcana 쪽)는 검정으로
q.addEventListener("input", () => {
  if (q.value.trim()) mode("typing");
  else if (!document.body.classList.contains("answering")) mode("open");
});

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") { e.preventDefault(); close(); }
});
// 빈 곳을 누르면 닫기 (글자·입력창은 제외)
window.addEventListener("mousedown", (e) => {
  if (!e.target.closest(".say, .ask")) close();
});
// 다른 창으로 가면 닫기
window.addEventListener("blur", () => { if (!document.body.classList.contains("closed")) close(); });

api.onQuick(() => open());
api.onEvent((e) => {
  if (e.kind === "thinking" && document.body.classList.contains("answering")) thinking(e.text);
});

// 브라우저로 미리보기할 때는 바로 열기
if (!window.__TAURI__) open();

// 빠른 호출 창 (기본 Alt+Shift+A)
// 위 = Arcana (파랑·보라 물결), 아래 = 사용자.
//  - 열면 사용자 쪽(아래)만 빛남. 나올 때 물결이 순간 빨라졌다가 Expo 로 원래 속도로
//  - Arcana가 생각·대답하는 동안: 위가 빛나고 아래는 연한 검정 (대답 중에 입력하면 둘 다 빛남)
//  - 빛나는 쪽이 바뀌면 그쪽 물결이 순간 빨라지고, 검정 쪽은 느려짐
//  - 사라질 때는 Expo 로 빨라지면서 사라짐
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
const clamp01 = (p) => Math.min(1, Math.max(0, p));
const outExpo = (p) => (p >= 1 ? 1 : 1 - Math.pow(2, -10 * p));
const inExpo = (p) => (p <= 0 ? 0 : Math.pow(2, 10 * p - 10));

// 값 하나를 시간에 따라 부드럽게 바꾸는 도구
class Tween {
  constructor(v) { this.from = this.to = v; this.start = 0; this.dur = 1; this.ease = outExpo; }
  set(to, dur, ease = outExpo, now = performance.now()) {
    this.from = this.value(now);
    this.to = to; this.start = now; this.dur = dur; this.ease = ease;
  }
  jump(v) { this.from = this.to = v; this.dur = 1; }
  value(now) { return lerp(this.from, this.to, this.ease(clamp01((now - this.start) / this.dur))); }
  done(now) { return now - this.start >= this.dur; }
}

// 순간적으로 빨라졌다가 Expo 로 원래 속도로 돌아오는 "속도 펄스"
class Kick {
  constructor() { this.amp = 0; this.start = 0; this.dur = 1; }
  fire(amp, dur, now = performance.now()) { this.amp = Math.max(amp, this.value(now)); this.start = now; this.dur = dur; }
  value(now) { return this.amp * (1 - outExpo(clamp01((now - this.start) / this.dur))); }
}

class Wave {
  constructor(canvas, side, palette) {
    this.c = canvas;
    this.x = canvas.getContext("2d");
    this.side = side;
    this.pal = palette.map(hex);
    this.lit = new Tween(0);     // 1 = 빛남(원래 색·위치), 0 = 연한 검정 + 가장자리로 내려감
    this.grow = new Tween(0);    // 0 = 숨김, 1 = 다 펼쳐짐 (나오기·사라지기)
    this.rest = new Tween(0.45); // 평소 물결 속도 (빛나면 1, 검정이면 느리게)
    this.kick = new Kick();      // 순간 가속
    this.exit = new Tween(0);    // 사라질 때 Expo 로 빨라지는 정도
    this.clock = Math.random() * 100;
    this.layers = [
      // dark = 검정일 때 남는 진하기 (가장자리 쪽 얇은 층일수록 연하게 → 굵은 검은 띠가 안 생김)
      { base: 0.62, amp: 0.10, freq: 1.1, speed: 0.42, phase: 0.0, alpha: 0.55, dark: 0.5 },
      { base: 0.48, amp: 0.12, freq: 1.7, speed: -0.6, phase: 1.7, alpha: 0.55, dark: 0.42 },
      { base: 0.36, amp: 0.09, freq: 2.4, speed: 0.78, phase: 3.1, alpha: 0.6, dark: 0.3 },
      { base: 0.24, amp: 0.07, freq: 3.1, speed: -0.95, phase: 4.4, alpha: 0.7, dark: 0.18 },
      { base: 0.13, amp: 0.05, freq: 4.2, speed: 1.15, phase: 0.9, alpha: 0.85, dark: 0.08 },
    ];
  }
  resize() {
    const s = 0.5 * (window.devicePixelRatio || 1); // 절반 해상도 → 가볍고 자연스럽게 번짐
    this.c.width = Math.max(1, Math.round(this.c.clientWidth * s));
    this.c.height = Math.max(1, Math.round(this.c.clientHeight * s));
  }
  // 빛나기 / 검정 바꾸기. 빛나는 쪽은 순간 빨라졌다가 돌아오고, 검정 쪽은 느려짐
  setLit(on, now = performance.now()) {
    if ((this.lit.to === 1) === on) return;
    this.lit.set(on ? 1 : 0, 750, outExpo, now);
    this.rest.set(on ? 1 : 0.45, 1100, outExpo, now);
    if (on) this.kick.fire(1.8, 1300, now);
  }
  enter(lit, now) {
    this.exit.jump(0);
    this.lit.jump(lit ? 1 : 0);
    this.rest.jump(lit ? 1 : 0.45);
    this.grow.jump(0);
    this.grow.set(1, 1100, outExpo, now);
    this.kick.fire(lit ? 4.5 : 3, 1600, now); // 나올 때: 순간 빨라졌다가 Expo 로 원래 속도
  }
  leave(now) {
    this.grow.set(0, 620, inExpo, now);   // 사라질 때: Expo 로 빨라지면서 사라짐
    this.exit.set(7, 620, inExpo, now);
  }
  color(i, a, m, dark) {
    const [r, g, b] = this.pal[i % this.pal.length];
    // 검정(비활성)일 때는 연하게, 살짝 남색 기운이 남게
    return `rgba(${lerp(4, r, m) | 0},${lerp(4, g, m) | 0},${lerp(12, b, m) | 0},${a * lerp(dark, 1, m)})`;
  }
  draw(now, dt) {
    const { x, c } = this;
    const w = c.width, h = c.height;
    const m = this.lit.value(now);
    const g = this.grow.value(now);
    this.clock += dt * (this.rest.value(now) + this.kick.value(now) + this.exit.value(now));
    const t = this.clock;
    x.clearRect(0, 0, w, h);
    if (g < 0.002) return;
    const top = this.side === "top";
    const steps = 72;
    // 비활성(검정)일 때는 물결 층이 화면 가장자리 쪽으로 내려가 얇아짐, 활성이면 원래 위치
    const sink = lerp(0.58, 1, m);
    const edgeY = (L, px) => {
      const u = px / w;
      // 천천히 숨 쉬듯 높이·세기가 바뀌는 물결 (여러 사인파를 겹쳐서 반복이 티 나지 않게)
      const breathe = 1 + 0.07 * Math.sin(t * 0.21 + L.phase * 2) + 0.04 * Math.sin(t * 0.53 + L.phase);
      const amp = L.amp * h * g * sink * (1 + 0.28 * Math.sin(t * 0.37 + L.phase));
      const y = L.base * h * g * sink * breathe
        + amp * Math.sin(u * Math.PI * 2 * L.freq + t * L.speed + L.phase)
        + amp * 0.45 * Math.sin(u * Math.PI * 2 * L.freq * 0.47 - t * L.speed * 0.63 + L.phase * 1.3)
        + amp * 0.18 * Math.sin(u * Math.PI * 2 * L.freq * 2.3 + t * L.speed * 1.7);
      return top ? y : h - y;
    };
    this.layers.forEach((L, i) => {
      x.beginPath();
      x.moveTo(-2, top ? -2 : h + 2);
      for (let k = 0; k <= steps; k++) {
        const px = (k / steps) * w;
        x.lineTo(px, edgeY(L, px));
      }
      x.lineTo(w + 2, top ? -2 : h + 2);
      x.closePath();
      // 물결 끝까지 투명해지게 해서 가장자리에 선이 생기지 않게
      const crest = (L.base * 1.12 + L.amp * 1.75) * h * g * sink;
      const grad = x.createLinearGradient(0, top ? 0 : h, 0, top ? crest : h - crest);
      grad.addColorStop(0, this.color(i, L.alpha, m, L.dark));
      grad.addColorStop(0.5, this.color(i + 1, L.alpha * 0.5, m, L.dark));
      grad.addColorStop(0.82, this.color(i + 2, L.alpha * 0.12, m, L.dark));
      grad.addColorStop(1, this.color(i + 2, 0, m, L.dark));
      x.fillStyle = grad;
      x.fill();
    });
  }
}

const top = new Wave($("#wave-top"), "top", TOP);
const bottom = new Wave($("#wave-bottom"), "bottom", BOTTOM);
let running = false;
let closing = false;
let lastFrame = 0;

function frame(now) {
  const dt = Math.min(0.05, (now - (lastFrame || now)) / 1000);
  lastFrame = now;
  top.draw(now, dt);
  bottom.draw(now, dt);
  const gone = closing && top.grow.done(now) && bottom.grow.done(now);
  if (gone) {
    running = false;
    lastFrame = 0;
    top.x.clearRect(0, 0, top.c.width, top.c.height);
    bottom.x.clearRect(0, 0, bottom.c.width, bottom.c.height);
    api.quickHide();
  } else requestAnimationFrame(frame);
}
function animate() {
  if (!running) { running = true; requestAnimationFrame(frame); }
}
function resize() { top.resize(); bottom.resize(); }
window.addEventListener("resize", resize);

// ───────────── 어느 쪽이 빛나는지 ─────────────
// 처음 열면 사용자 쪽(아래)만 · Arcana가 생각·대답하는 동안은 위만 · 둘이 동시에(대답 중에 입력) 하면 둘 다
const st = { aiBusy: false, shown: false, typing: false };
function update() {
  const now = performance.now();
  const aiLit = st.aiBusy || (st.shown && !st.typing);
  const userLit = st.typing || (!st.aiBusy && !st.shown);
  top.setLit(aiLit, now);
  bottom.setLit(userLit, now);
  document.body.classList.toggle("answering", st.aiBusy);
}

let token = 0; // 닫았다 열면 이전 대답은 무시
let pendingConfirm = null;
const box = textbox(q, (v) => submit(v), { maxHeight: 220 });

function open() {
  token++;
  pendingConfirm = null;
  reply.textContent = "";
  reply.className = "reply";
  status.textContent = "";
  sent.textContent = "";
  box.clear();
  resize();
  closing = false;
  document.body.classList.remove("closed");
  Object.assign(st, { aiBusy: false, shown: false, typing: false });
  const now = performance.now();
  top.enter(false, now);   // Arcana 쪽은 검정으로 시작
  bottom.enter(true, now); // 사용자 쪽만 빛남
  update();
  animate();
  setTimeout(() => q.focus(), 30);
}

function close() {
  if (closing) return;
  token++;
  closing = true;
  document.body.classList.add("closed");
  const now = performance.now();
  top.leave(now);
  bottom.leave(now);
  q.blur();
  animate(); // 사라지는 애니메이션이 끝나면 창을 숨김
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
  // 길이에 맞춰 글자 크기를 미리 정해서 칸을 넘지 않게
  const n = [...text].length + (text.match(/\n/g)?.length ?? 0) * 40;
  reply.classList.toggle("long", n > 140 && n <= 360);
  reply.classList.toggle("longer", n > 360);
  reply.classList.remove("overflow");
  const fit = () => {
    reply.classList.toggle("overflow", reply.scrollHeight > reply.clientHeight + 2);
    reply.scrollTop = reply.scrollHeight;
  };
  await typeOut(reply, text, { render: linkify, onTick: fit });
  reply.scrollTop = 0; // 다 쓰면 처음부터 읽을 수 있게
  if (mine !== token) return;
  st.aiBusy = false;
  st.shown = true;
  update();
}

function busy() {
  st.aiBusy = true;
  st.shown = false;
  update();
}

async function submit(raw) {
  const text = raw.trim();
  const mine = token;
  // "진행할까요?" 질문 뒤에 그냥 Enter = 예
  if (pendingConfirm && !text) {
    const intent = pendingConfirm;
    pendingConfirm = null;
    busy();
    thinking();
    const r = await api.confirm(intent);
    if (mine === token) await say(r.text, mine);
    return;
  }
  if (!text || st.aiBusy) return;
  pendingConfirm = null;
  sent.textContent = text;
  box.clear();
  st.typing = false;
  busy();
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

// 입력하기 시작하면 사용자 쪽이 빛나고, 대답이 끝난 Arcana 쪽은 검정으로
q.addEventListener("input", () => {
  st.typing = !!q.value.trim();
  if (st.typing && !st.aiBusy) st.shown = false;
  update();
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

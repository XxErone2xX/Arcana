// 빠른 호출 창 (기본 Alt+Shift+A)
// 위 = Arcana (파랑·보라 물결), 아래 = 사용자.
//  - 열면 둘 다 검정, 입력을 시작하면 사용자 쪽(아래)이 빛남. 나올 때 물결이 순간 빨라졌다가 Expo 로 원래 속도로
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
  value(now) {
    const p = clamp01((now - this.start) / this.dur);
    return p >= 1 ? 0 : this.amp * (Math.pow(2, -6 * p) - Math.pow(2, -6)) / (1 - Math.pow(2, -6)); // Expo 감쇠
  }
}

// 속도 (평소 = 1배). 눈에 확 보이도록 크게 잡음
const DARK_SPEED = 0.22;  // 검정 쪽: 확실히 느리게
const SWITCH_KICK = 5;    // 빛나는 쪽으로 바뀔 때: 순간 6배까지 빨라졌다가 Expo 로 돌아옴
const OPEN_KICK = 10;     // 나올 때: 순간 11배까지
const CLOSE_SPEED = 22;   // 사라질 때: 끝에 22배까지 빨라짐

class Wave {
  constructor(canvas, side, palette) {
    this.c = canvas;
    this.x = canvas.getContext("2d");
    this.side = side;
    this.pal = palette.map(hex);
    this.lit = new Tween(0);     // 1 = 빛남(원래 색·위치), 0 = 연한 검정 + 가장자리로 내려감
    this.rest = new Tween(DARK_SPEED); // 평소 물결 속도 (빛나면 1, 검정이면 느리게)
    this.kick = new Kick();      // 순간 가속
    this.exit = new Tween(0);    // 사라질 때 Expo 로 빨라지는 정도
    this.clock = Math.random() * 100;
    this.layers = [
      // dark = 검정일 때 남는 진하기 (가장자리 쪽 얇은 층일수록 연하게 → 굵은 검은 띠가 안 생김)
      { base: 0.62, amp: 0.10, freq: 1.1, speed: 0.6, phase: 0.0, alpha: 0.55, dark: 0.5 },
      { base: 0.48, amp: 0.12, freq: 1.7, speed: -0.85, phase: 1.7, alpha: 0.55, dark: 0.42 },
      { base: 0.36, amp: 0.09, freq: 2.4, speed: 1.1, phase: 3.1, alpha: 0.6, dark: 0.3 },
      { base: 0.24, amp: 0.07, freq: 3.1, speed: -1.3, phase: 4.4, alpha: 0.7, dark: 0.18 },
      { base: 0.13, amp: 0.05, freq: 4.2, speed: 1.6, phase: 0.9, alpha: 0.85, dark: 0.08 },
    ];
    for (const L of this.layers) {
      L.slide = new Tween(0); // 1 = 제자리, 0 = 화면 밖 (나오기·사라지기)
      L.drop = new Tween(1);  // 1 = 검정이라 가장자리 쪽으로 내려가 있음, 0 = 원래 위치
    }
  }
  resize() {
    const s = 0.5 * (window.devicePixelRatio || 1); // 절반 해상도 → 가볍고 자연스럽게 번짐
    this.c.width = Math.max(1, Math.round(this.c.clientWidth * s));
    this.c.height = Math.max(1, Math.round(this.c.clientHeight * s));
  }
  // 빛나기 / 검정 바꾸기. 빛나는 쪽은 순간 빨라졌다가 돌아오고, 검정 쪽은 느려짐
  // 층마다 조금씩 시간차를 두고, 각 층이 통째로 가장자리 쪽(검정) / 원래 위치(빛남)로 이동
  setLit(on, now = performance.now()) {
    if ((this.lit.to === 1) === on) return;
    this.lit.set(on ? 1 : 0, 750, outExpo, now);
    this.rest.set(on ? 1 : DARK_SPEED, 1400, outExpo, now);
    if (on) this.kick.fire(SWITCH_KICK, 2000, now);
    this.layers.forEach((L, i) => {
      const order = on ? this.layers.length - 1 - i : i; // 빛날 땐 가장자리 층부터 올라옴
      L.drop.set(on ? 0 : 1, 900, outExpo, now + order * 60);
    });
  }
  // 나타나기: 층이 하나씩 화면 밖에서 밀려 들어옴 (가장자리 층부터)
  enter(lit, now) {
    this.exit.jump(0);
    this.lit.jump(lit ? 1 : 0);
    this.rest.jump(lit ? 1 : DARK_SPEED);
    const n = this.layers.length;
    this.layers.forEach((L, i) => {
      L.drop.jump(lit ? 0 : 1);
      L.slide.jump(0);
      L.slide.set(1, 1200, outExpo, now + (n - 1 - i) * 70);
    });
    this.kick.fire(OPEN_KICK * (lit ? 1 : 0.6), 2400, now); // 나올 때: 순간 빨라졌다가 Expo 로 원래 속도
  }
  // 사라지기: 안쪽 층부터 하나씩 화면 밖으로 빠져나감, Expo 로 빨라지면서
  leave(now) {
    this.layers.forEach((L, i) => L.slide.set(0, 650, inExpo, now + i * 45));
    this.exit.set(CLOSE_SPEED, 650 + this.layers.length * 45, inExpo, now);
  }
  gone(now) { return this.layers.every((L) => L.slide.to === 0 && L.slide.done(now)); }
  color(i, a, m, dark) {
    const [r, g, b] = this.pal[i % this.pal.length];
    // 검정(비활성)일 때는 연하게, 살짝 남색 기운이 남게
    return `rgba(${lerp(4, r, m) | 0},${lerp(4, g, m) | 0},${lerp(12, b, m) | 0},${a * lerp(dark, 1, m)})`;
  }
  draw(now, dt) {
    const { x, c } = this;
    const w = c.width, h = c.height;
    const m = this.lit.value(now);
    this.clock += dt * (this.rest.value(now) + this.kick.value(now) + this.exit.value(now));
    const t = this.clock;
    x.clearRect(0, 0, w, h);
    const top = this.side === "top";
    const steps = 72;
    // 모든 계산은 "화면 가장자리에서 얼마나 떨어졌나(d)"로 하고, 위/아래에 맞춰 뒤집음
    const Y = (d) => (top ? d : h - d);
    this.layers.forEach((L, i) => {
      // 층 하나를 모양 그대로 통째로 이동 (늘이거나 줄이지 않음)
      const reach = (L.base * 1.12 + L.amp * 1.75) * h;
      const off = (1 - L.slide.value(now)) * (reach + 4) + L.drop.value(now) * 0.42 * L.base * h;
      if (off >= reach) return; // 완전히 화면 밖
      const amp = L.amp * h * (1 + 0.28 * Math.sin(t * 0.37 + L.phase));
      const breathe = 1 + 0.07 * Math.sin(t * 0.21 + L.phase * 2) + 0.04 * Math.sin(t * 0.53 + L.phase);
      const depth = (u) => L.base * h * breathe
        + amp * Math.sin(u * Math.PI * 2 * L.freq + t * L.speed + L.phase)
        + amp * 0.45 * Math.sin(u * Math.PI * 2 * L.freq * 0.47 - t * L.speed * 0.63 + L.phase * 1.3)
        + amp * 0.18 * Math.sin(u * Math.PI * 2 * L.freq * 2.3 + t * L.speed * 1.7)
        - off;
      x.beginPath();
      x.moveTo(-2, Y(-2));
      for (let k = 0; k <= steps; k++) x.lineTo((k / steps) * w, Y(depth(k / steps)));
      x.lineTo(w + 2, Y(-2));
      x.closePath();
      // 색도 층과 같이 이동, 물결 끝까지 투명해지게 해서 가장자리에 선이 생기지 않게
      const grad = x.createLinearGradient(0, Y(-off), 0, Y(reach - off));
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
window.__waves = { top, bottom }; // 디버그용
let running = false;
let closing = false;
let lastFrame = 0;

function frame(now) {
  const dt = Math.min(0.05, (now - (lastFrame || now)) / 1000);
  lastFrame = now;
  top.draw(now, dt);
  bottom.draw(now, dt);
  const gone = closing && top.gone(now) && bottom.gone(now);
  if (gone) {
    running = false;
    lastFrame = 0;
    top.x.clearRect(0, 0, top.c.width, top.c.height);
    bottom.x.clearRect(0, 0, bottom.c.width, bottom.c.height);
    reset();
    q.focus(); // 숨겨진 동안에도 입력창이 포커스를 갖고 있어서, 다시 뜨면 바로 입력됨
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
  const userLit = st.typing; // 입력을 시작해야 빛남
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
  // 입력창은 비우지 않음 (창이 뜨자마자 친 글자가 지워지지 않게 — 비우는 건 닫을 때)
  resize();
  closing = false;
  document.body.classList.remove("closed");
  Object.assign(st, { aiBusy: false, shown: false, typing: !!q.value.trim() });
  const now = performance.now();
  top.enter(false, now);    // 처음엔 둘 다 검정
  bottom.enter(false, now);
  update();
  animate();
  window.focus();
  q.focus();
}

// 다음에 열 때를 위해 깨끗하게 (닫힌 뒤에)
function reset() {
  reply.textContent = "";
  reply.className = "reply";
  status.textContent = "";
  sent.textContent = "";
  box.clear();
  Object.assign(st, { aiBusy: false, shown: false, typing: false });
}

function close() {
  if (closing) return;
  token++;
  closing = true;
  document.body.classList.add("closed");
  const now = performance.now();
  top.leave(now);
  bottom.leave(now);
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
  if (e.key === "Escape") { e.preventDefault(); close(); return; }
  // 입력창 말고 다른 곳에 포커스가 있어도 글자를 치면 입력창으로 (첫 글자가 안 먹는 문제)
  if (document.activeElement !== q && !e.ctrlKey && !e.metaKey && !e.altKey && (e.key.length === 1 || e.key === "Process")) q.focus();
}, true);
// 단축키(Alt+Shift+A)에서 손을 뗄 때 Alt 때문에 창 메뉴가 잡혀 다음 글자가 먹히는 것 막기
window.addEventListener("keyup", (e) => { if (e.key === "Alt") e.preventDefault(); }, true);
window.addEventListener("keydown", (e) => { if (e.key === "Alt") e.preventDefault(); }, true);
window.addEventListener("focus", () => { if (!closing) q.focus(); });
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

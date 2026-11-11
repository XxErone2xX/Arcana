// Rust 쪽(app/src/main.rs)과 통신하는 부분. UI 코드는 이 파일의 함수만 쓰면 됨.
// 브라우저에서 index.html을 그냥 열면(Tauri 없이) 가짜 응답으로 UI만 미리 볼 수 있음.

const T = window.__TAURI__;
export const isApp = !!T;

const invoke = (cmd, args) => T.core.invoke(cmd, args);

export const api = isApp
  ? {
      send: (text) => invoke("send", { text }),
      confirm: (intent) => invoke("confirm", { intent }),
      study: (text) => invoke("study", { text }),
      studyStatus: () => invoke("study_status"),
      openUrl: (url) => invoke("open_url", { url }),
      feedback: (user, reply, rating, better) => invoke("feedback", { user, reply, rating, better }),
      info: () => invoke("info"),
      setOption: (key, value) => invoke("set_option", { key, value }),
      openConfig: () => invoke("open_config"),
      reloadConfig: () => invoke("reload_config"),
      checkUpdate: () => invoke("check_update"),
      applyUpdate: () => invoke("apply_update"),
      setValue: (key, value) => invoke("set_value", { key, value }),
      memoryList: () => invoke("memory_list"),
      memoryAdd: (text) => invoke("memory_add", { text }),
      memoryRemove: (id) => invoke("memory_remove", { id }),
      memoryClear: () => invoke("memory_clear"),
      containerInfo: () => invoke("container_info"),
      containerClear: () => invoke("container_clear"),
      quickHide: () => invoke("quick_hide"),
      apiTest: () => invoke("api_test"),
      onQuick: (cb) => T.event.listen("arcana://quick", (e) => cb(e.payload)),
      onUpdating: (cb) => T.event.listen("arcana://updating", (e) => cb(e.payload)),
      onUpdateFailed: (cb) => T.event.listen("arcana://update-failed", (e) => cb(e.payload)),
      onEvent: (cb) => T.event.listen("arcana://event", (e) => cb(e.payload)),
      window: {
        minimize: () => T.window.getCurrentWindow().minimize(),
        close: () => T.window.getCurrentWindow().close(),
      },
    }
  : mock();

// ───── UI 미리보기용 가짜 API ─────
function mock() {
  const listeners = [];
  return {
    async send(text) {
      await new Promise((r) => setTimeout(r, 600));
      const ask = /꺼줘|지워/.test(text) && /컴퓨터|메모/.test(text);
      return {
        understanding: { intent: "preview", confidence: 0.96, slots: {} },
        fallback: false,
        reply: { text: text.includes("길게") ? "(미리보기) 긴 대답 시험입니다. ".repeat(30) : `(미리보기) "${text}"에 대한 대답입니다. 대답은 이렇게 한 글자씩 써집니다. 링크도 됩니다: https://ko.wikipedia.org`, confirm: ask ? "진행할까요?" : null, quit: false, error: false, from_chat: !ask },
      };
    },
    async confirm() { return { text: "(미리보기) 확인했습니다.", error: false }; },
    async study(text) {
      const running = /start/i.test(text) ? true : /stop/i.test(text) ? false : window.__running ?? false;
      window.__running = running;
      return { lines: text ? [`(미리보기) ${text}`] : [], running, question: "'수성'은(는) 뜻이 여러 개입니다. 어떤 걸 공부할까요? (번호 또는 이름)\n1. 수성 (행성)\n2. 수성 (화학)", stats: { count: 12, queued: 80, taught: 1, questions: 1, recent: ["블랙홀"] } };
    },
    async studyStatus() { return this.study(""); },
    async openUrl(url) { window.open(url, "_blank"); },
    async feedback(u, r, rating, better) { return rating > 0 ? "감사합니다. 이런 대답을 더 자주 하겠습니다." : better ? "알겠습니다. 다음부터는 알려 주신 대로 대답하겠습니다." : "죄송합니다. 그 대답은 피하겠습니다."; },
    async info() {
      if (window.__fail > 0) { window.__fail--; throw new Error("아직 준비 중"); } // 시험용: 켜지는 중 흉내
      return window.__info ??= {
        name: "Arcana", app_name: "Arcana", version: "preview", brain: "mock", talker: "mock", show_debug: true, auto_update: true,
        data_dir: "-", missing_skills: [],
        capabilities: ["프로그램 켜기/끄기 — \"디코 켜줘\"", "타이머 — \"라면 3분 타이머\""],
        think_level: 3, memory_enabled: true, container_enabled: true, container_mb: 100,
        hotkey: "Alt+Shift+A", hotkey_enabled: true, hotkey_error: null,
        api_provider: "", api_model: "", api_base: "", api_mode: "fallback", api_key_hint: "", api_enabled: false,
        auto_train: true, obey_robots: false, auto_train_idle_min: 10, auto_train_status: "자동 학습 켜짐 · 대화 재료 12쌍 · PC를 10분 안 쓰면 학습 · 아직 재료 부족 (대화 데이터 또는 가르친 대답 30개 필요)",
      };
    },
    async setValue(k, v) { (await this.info())[k] = v; },
    async memoryList() { return window.__mem ??= [{ id: 1, kind: "name", value: "얼로니", created: "2026-11-07" }, { id: 2, kind: "like", value: "민트초코", created: "2026-11-07" }]; },
    async memoryAdd(text) { const f = { id: Date.now(), kind: "note", value: text, created: "2026-11-07" }; (await this.memoryList()).push(f); return f; },
    async memoryRemove(id) { window.__mem = (await this.memoryList()).filter((f) => f.id !== id); },
    async memoryClear() { window.__mem = []; },
    async containerInfo() { return { used_bytes: 3_400_000, max_mb: 100, enabled: true, turns: 50 }; },
    async containerClear() {},
    async quickHide() { document.body.classList.add("closed"); },
    async apiTest() { await new Promise((r) => setTimeout(r, 500)); return "(미리보기) 연결 성공: 연결되었습니다."; },
    onQuick() {},
    onUpdating() {},
    onUpdateFailed() {},
    async setOption() {}, async openConfig() {}, async reloadConfig() {},
    async checkUpdate() { return { available: false, current: "preview", latest: "preview" }; },
    async applyUpdate() {},
    onEvent(cb) { listeners.push(cb); window.__mockEvent = (e) => listeners.forEach((l) => l(e)); },
    window: { minimize() {}, close() { alert("닫기"); } },
  };
}

// 앱이 켜지는 동안(두뇌·대화 모델·공책을 불러오는 몇 초) Rust 쪽이 아직 준비가 안 됐을 수 있음.
// 그때 부르면 실패하므로, 준비될 때까지 잠깐씩 기다리며 다시 시도함.
let readyPromise = null;
export function ready() {
  readyPromise ??= (async () => {
    for (let i = 0; ; i++) {
      try {
        return await api.info();
      } catch (e) {
        if (i > 600) throw e; // 약 1분
        await new Promise((r) => setTimeout(r, 100));
      }
    }
  })();
  return readyPromise;
}

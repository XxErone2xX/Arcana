// Rust 쪽(app/src/main.rs)과 통신하는 부분. UI 코드는 이 파일의 함수만 쓰면 됨.
// 브라우저에서 index.html을 그냥 열면(Tauri 없이) 가짜 응답으로 UI만 미리 볼 수 있음.

const T = window.__TAURI__;
export const isApp = !!T;

const invoke = (cmd, args) => T.core.invoke(cmd, args);

export const api = isApp
  ? {
      send: (text) => invoke("send", { text }),
      confirm: (intent) => invoke("confirm", { intent }),
      info: () => invoke("info"),
      setOption: (key, value) => invoke("set_option", { key, value }),
      openConfig: () => invoke("open_config"),
      reloadConfig: () => invoke("reload_config"),
      checkUpdate: () => invoke("check_update"),
      applyUpdate: () => invoke("apply_update"),
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
      await new Promise((r) => setTimeout(r, 250));
      const ask = /꺼줘|지워/.test(text) && /컴퓨터|메모/.test(text);
      return {
        understanding: { intent: "preview", confidence: 0.96, slots: {} },
        fallback: false,
        reply: { text: `(미리보기) "${text}"`, confirm: ask ? "진행할까요?" : null, quit: false, error: false },
      };
    },
    async confirm() { return { text: "(미리보기) 확인했습니다.", error: false }; },
    async info() {
      return {
        name: "Arcana", app_name: "Arcana", version: "preview", brain: "mock", talker: "mock", show_debug: true, auto_update: true,
        data_dir: "-", missing_skills: [],
        capabilities: ["프로그램 켜기/끄기 — \"디코 켜줘\"", "타이머 — \"라면 3분 타이머\""],
      };
    },
    async setOption() {}, async openConfig() {}, async reloadConfig() {},
    async checkUpdate() { return { available: false, current: "preview", latest: "preview" }; },
    async applyUpdate() {},
    onEvent(cb) { listeners.push(cb); window.__mockEvent = (e) => listeners.forEach((l) => l(e)); },
    window: { minimize() {}, close() { alert("닫기"); } },
  };
}

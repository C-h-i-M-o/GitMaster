import assert from "node:assert/strict";
import test from "node:test";
import {
  EditorAutosaveController,
  type EditorAutosaveSnapshot,
} from "../src/hooks/editorAutosaveController.ts";

const wait = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));
function setup(
  initial: EditorAutosaveSnapshot,
  save: (path: string) => Promise<boolean>,
) {
  let snapshot = initial;
  const controller = new EditorAutosaveController({
    getSnapshot: () => snapshot,
    save,
    blocked: () => false,
    mode: "auto",
    delayMs: 10,
  });
  return {
    controller,
    set: (next: EditorAutosaveSnapshot) => {
      snapshot = next;
    },
  };
}
function markSaved(
  state: { set: (next: EditorAutosaveSnapshot) => void },
  snapshot: EditorAutosaveSnapshot,
  path: string,
): void {
  state.set({
    documents: snapshot.documents.map((document) =>
      document.path === path
        ? { ...document, baseline: document.draft }
        : document,
    ),
  });
}

test("800ms per document and background documents are queued", async () => {
  const calls: string[] = [];
  const state = setup(
    {
      documents: [
        { path: "a", draft: "2", baseline: "1" },
        { path: "b", draft: "2", baseline: "1" },
      ],
    },
    async (path) => {
      calls.push(path);
      markSaved(state, current(), path);
      return true;
    },
  );
  const current = (): EditorAutosaveSnapshot => ({
    documents: [
      { path: "a", draft: "2", baseline: calls.includes("a") ? "2" : "1" },
      { path: "b", draft: "2", baseline: calls.includes("b") ? "2" : "1" },
    ],
  });
  state.controller.notifyChanged("a");
  state.controller.notifyChanged("b");
  await wait(25);
  assert.deepEqual(calls.sort(), ["a", "b"]);
  state.controller.dispose();
});

test("new edit during in-flight save remains dirty and schedules again", async () => {
  let resolve!: (value: boolean) => void;
  const calls: string[] = [];
  const state = setup(
    { documents: [{ path: "a", draft: "2", baseline: "1" }] },
    async (path) => {
      calls.push(path);
      if (calls.length > 1) {
        state.set({ documents: [{ path, draft: "3", baseline: "3" }] });
        return true;
      }
      return new Promise((done) => {
        resolve = (value) => {
          state.set({
            documents: [{ path, draft: "3", baseline: value ? "2" : "1" }],
          });
          done(value);
        };
      });
    },
  );
  state.controller.notifyChanged("a");
  await wait(15);
  state.set({ documents: [{ path: "a", draft: "3", baseline: "1" }] });
  resolve(true);
  await wait(20);
  assert.deepEqual(calls, ["a", "a"]);
  state.controller.dispose();
});

test("snapshot 引用只在 emit 后变化", () => {
  const state = setup({ documents: [] }, async () => true);
  const first = state.controller.getSnapshot();
  let notified = 0;
  state.controller.subscribe(() => {
    notified += 1;
  });
  state.controller.compositionStart();
  const second = state.controller.getSnapshot();
  assert.notEqual(first, second);
  assert.equal(notified, 1);
  assert.equal(state.controller.getSnapshot(), second);
  state.controller.dispose();
});

test("flush waits for delayed in-flight save without microtask spinning", async () => {
  let resolve!: (value: boolean) => void;
  const state = setup(
    { documents: [{ path: "a", draft: "2", baseline: "1" }] },
    async () =>
      new Promise((done) => {
        resolve = (value) => {
          state.set({
            documents: [{ path: "a", draft: "2", baseline: value ? "2" : "1" }],
          });
          done(value);
        };
      }),
  );
  state.controller.notifyChanged("a");
  await wait(15);
  let finished = false;
  const pending = state.controller.flush().then(() => {
    finished = true;
  });
  await wait(5);
  assert.equal(finished, false);
  resolve(true);
  await pending;
  state.controller.dispose();
});

test("blocked解除后自动保存", async () => {
  let blocked = true;
  let count = 0;
  const snapshot = { documents: [{ path: "a", draft: "2", baseline: "1" }] };
  let current = snapshot;
  const controller = new EditorAutosaveController({
    getSnapshot: () => current,
    save: async () => {
      count += 1;
      current = { documents: [{ path: "a", draft: "2", baseline: "2" }] };
      return true;
    },
    blocked: () => blocked,
    mode: "auto",
    delayMs: 5,
  });
  controller.notifyChanged("a");
  await wait(12);
  assert.equal(count, 0);
  blocked = false;
  await wait(15);
  assert.equal(count, 1);
  controller.dispose();
});

test("retry 与自动任务并发时最多一个写入", async () => {
  let active = 0;
  let maximum = 0;
  let resolve!: (value: boolean) => void;
  const state = setup(
    { documents: [{ path: "a", draft: "2", baseline: "1" }] },
    async () => {
      active += 1;
      maximum = Math.max(maximum, active);
      const result = await new Promise<boolean>((done) => {
        resolve = (value) => {
          if (value)
            state.set({
              documents: [{ path: "a", draft: "2", baseline: "2" }],
            });
          done(value);
        };
      });
      active -= 1;
      return result;
    },
  );
  state.controller.notifyChanged("a");
  await wait(15);
  const retry = state.controller.retry("a");
  resolve(true);
  await retry;
  assert.equal(maximum, 1);
  state.controller.dispose();
});

test("dispose后不计时、不通知迟到结果，手动模式取消自动任务", async () => {
  let notifications = 0;
  let resolve!: (value: boolean) => void;
  const state = setup(
    { documents: [{ path: "a", draft: "2", baseline: "1" }] },
    async () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  state.controller.subscribe(() => {
    notifications += 1;
  });
  state.controller.notifyChanged("a");
  state.controller.setMode("manual");
  await wait(15);
  assert.equal(notifications, 0);
  state.controller.setMode("auto");
  state.controller.notifyChanged("a");
  await wait(15);
  state.controller.dispose();
  const before = notifications;
  resolve(true);
  await wait(5);
  assert.equal(notifications, before);
});

test("failed document is blocked until retry", async () => {
  let count = 0;
  const state = setup(
    { documents: [{ path: "a", draft: "2", baseline: "1" }] },
    async () => {
      count += 1;
      return false;
    },
  );
  state.controller.notifyChanged("a");
  await wait(15);
  state.set({ documents: [{ path: "a", draft: "3", baseline: "1" }] });
  state.controller.notifyChanged("a");
  await wait(15);
  assert.equal(count, 1);
  state.controller.dispose();
});

test("composition and manual mode cancel timers; flush still saves dirty docs", async () => {
  const calls: string[] = [];
  const state = setup(
    { documents: [{ path: "a", draft: "2", baseline: "1" }] },
    async (path) => {
      calls.push(path);
      state.set({
        documents: [
          {
            path,
            draft: calls.length === 1 ? "2" : "3",
            baseline: calls.length === 1 ? "2" : "3",
          },
        ],
      });
      return true;
    },
  );
  state.controller.compositionStart();
  state.controller.notifyChanged("a");
  await wait(15);
  assert.equal(calls.length, 0);
  state.controller.compositionEnd();
  await wait(15);
  assert.equal(calls.length, 1);
  state.controller.setMode("manual");
  state.set({ documents: [{ path: "a", draft: "3", baseline: "1" }] });
  await state.controller.flush();
  assert.equal(calls.length, 2);
  state.controller.dispose();
});

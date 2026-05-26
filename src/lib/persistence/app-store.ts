import { load } from "@tauri-apps/plugin-store";
import type { PersistedAppState } from "$lib/state/app-state.svelte";

let storePromise: ReturnType<typeof load> | null = null;

async function getStore() {
  if (!storePromise) {
    storePromise = load("app-state.json", { defaults: {}, autoSave: 250 });
  }
  return storePromise;
}

export async function loadPersistedAppState(): Promise<PersistedAppState | null> {
  const store = await getStore();
  return (await store.get<PersistedAppState>("app-state")) ?? null;
}

export async function savePersistedAppState(state: PersistedAppState): Promise<void> {
  const store = await getStore();
  await store.set("app-state", state);
  await store.save();
}

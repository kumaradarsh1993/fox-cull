import { Store } from "@tauri-apps/plugin-store";

let store: Store | null = null;
async function s(): Promise<Store> {
  if (!store) store = await Store.load("fox-cull.json");
  return store;
}

export async function getLastRoot(): Promise<string | null> {
  return (await (await s()).get<string>("root")) ?? null;
}

export async function setLastRoot(root: string): Promise<void> {
  const st = await s();
  await st.set("root", root);
  await st.save();
}

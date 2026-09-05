import { writable, derived, get } from "svelte/store";
import en from "../locales/en.json";
import fa from "../locales/fa.json";

export type Lang = "en" | "fa";

const bundles: Record<Lang, Record<string, string>> = { en, fa };

export const lang = writable<Lang>("en");

/** Translate a key; falls back to the key itself when missing. */
export function t(key: string): string {
  return bundles[get(lang)][key] ?? key;
}

/** Reactive translator for templates: `$tt('connect')`. */
export const tt = derived(lang, ($lang) => {
  return (key: string): string => bundles[$lang][key] ?? key;
});

export function applyDir(l: Lang) {
  document.documentElement.lang = l;
  document.documentElement.dir = l === "fa" ? "rtl" : "ltr";
}

lang.subscribe(applyDir);

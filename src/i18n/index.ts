/**
 * i18n store — reactive locale switching with localStorage persistence.
 *
 * Usage:
 *   import { t, setLocale, getLocale } from "../i18n";
 *   <span>{t("settingsTitle")}</span>
 *   setLocale("en");
 */
import { createSignal } from "solid-js";
import zhCN from "./locales/zh-CN";
import en from "./locales/en";
import type { Translations, TranslationKey } from "./locales/zh-CN";

export type Locale = "zh-CN" | "en";

const STORAGE_KEY = "bentodesk-locale";

const locales: Record<Locale, Translations> = { "zh-CN": zhCN, en };

const [currentLocale, setCurrentLocale] = createSignal<Locale>("zh-CN");

// Restore saved locale on module load
const saved = localStorage.getItem(STORAGE_KEY) as Locale | null;
if (saved && locales[saved]) {
  setCurrentLocale(saved);
}

/**
 * Translate a key to the current locale's string.
 * Because this reads a Solid.js signal, it is reactive —
 * any component calling `t(key)` will re-render when the locale changes.
 */
export function t(key: TranslationKey): string {
  return locales[currentLocale()][key];
}

/**
 * Switch the active locale and persist to localStorage.
 */
export function setLocale(locale: Locale): void {
  setCurrentLocale(locale);
  localStorage.setItem(STORAGE_KEY, locale);
}

/**
 * Read the current locale (reactive).
 */
export function getLocale(): Locale {
  return currentLocale();
}

/**
 * SettingsPanel — Full overlay settings panel with glassmorphism.
 * Groups: General, Appearance, Performance.
 * Toggle switches, sliders, color picker.
 * Escape to close.
 */
import {
  Component,
  Show,
  For,
  createSignal,
  onMount,
  onCleanup,
  createEffect,
} from "solid-js";
import { isSettingsPanelOpen, closeSettingsPanel } from "../../stores/ui";
import {
  getSettings,
  updateSettings as updateSettingsStore,
} from "../../stores/settings";
import {
  plugins,
  pluginsLoading,
  loadPlugins,
  installPluginAction,
  uninstallPluginAction,
  togglePluginAction,
} from "../../stores/plugins";
import { t, getLocale, setLocale } from "../../i18n";
import type { Locale } from "../../i18n";
import type { TranslationKey } from "../../i18n/locales/zh-CN";
import type { AppSettings, SettingsUpdate } from "../../types/settings";
import type { InstalledPlugin, PluginType } from "../../types/plugins";
import type { DesktopSourceInfo, DesktopSourceKind } from "../../types/system";
import { getDesktopSources } from "../../services/ipc";
import {
  getAvailableThemes,
  getThemeId,
  setTheme,
  importThemeFromJSON,
  exportThemeAsJSON,
} from "../../themes";
import type { BentoTheme } from "../../themes";
import { open } from "@tauri-apps/plugin-dialog";
import "./SettingsPanel.css";

/**
 * Ordered theme IDs grouped by visual family. Any ID returned by
 * getAvailableThemes() but not listed here (custom themes) falls into a
 * trailing "Personality" bucket so nothing is dropped from the UI.
 */
const THEME_GROUP_ORDER: ReadonlyArray<{ key: TranslationKey; ids: readonly string[] }> = [
  {
    key: "themeGroupRounded",
    ids: ["dark", "light", "midnight", "forest", "sunset", "frosted", "ocean-blue", "rose-gold", "forest-green"],
  },
  { key: "themeGroupSolid", ids: ["solid"] },
  { key: "themeGroupAngular", ids: ["order", "flat", "brutalism", "editorial"] },
  { key: "themeGroupPersonality", ids: ["neo", "terminal", "cyberpunk"] },
];

function groupThemes(all: readonly BentoTheme[]): ReadonlyArray<{ key: TranslationKey; themes: BentoTheme[] }> {
  const byId = new Map(all.map((t) => [t.id, t]));
  const seen = new Set<string>();
  const groups: Array<{ key: TranslationKey; themes: BentoTheme[] }> = THEME_GROUP_ORDER.map(
    ({ key, ids }) => {
      const themes = ids
        .map((id) => byId.get(id))
        .filter((t): t is BentoTheme => {
          if (!t) return false;
          seen.add(t.id);
          return true;
        });
      return { key, themes };
    }
  );
  const leftover = all.filter((t) => !seen.has(t.id));
  if (leftover.length > 0) {
    const personality = groups.find((g) => g.key === "themeGroupPersonality");
    if (personality) {
      personality.themes = [...personality.themes, ...leftover];
    }
  }
  return groups.filter((g) => g.themes.length > 0);
}

function desktopSourceLabelKey(kind: DesktopSourceKind): TranslationKey {
  switch (kind) {
    case "user":
      return "desktopSourceUser";
    case "public":
      return "desktopSourcePublic";
    case "onedrive":
      return "desktopSourceOneDrive";
    case "custom":
      return "desktopSourceCustom";
  }
}

function desktopSourceInitial(kind: DesktopSourceKind): string {
  switch (kind) {
    case "user":
      return "U";
    case "public":
      return "P";
    case "onedrive":
      return "O";
    case "custom":
      return "C";
  }
}

const SettingsPanel: Component = () => {
  const [localSettings, setLocalSettings] = createSignal<AppSettings>(
    getSettings()
  );
  const [dirty, setDirty] = createSignal(false);

  // Developer theme section state
  const [devOpen, setDevOpen] = createSignal(false);
  const [themeJson, setThemeJson] = createSignal("");
  const [themeFeedback, setThemeFeedback] = createSignal<{
    type: "success" | "error";
    message: string;
  } | null>(null);

  // Plugin uninstall confirmation
  const [confirmingUninstall, setConfirmingUninstall] = createSignal<string | null>(null);

  // Desktop sources (multi-source). Populated via invoke("get_desktop_sources")
  // when the panel opens, and on demand via the refresh button.
  const [desktopSources, setDesktopSources] = createSignal<DesktopSourceInfo[]>([]);
  const [desktopSourcesLoading, setDesktopSourcesLoading] = createSignal(false);

  const refreshDesktopSources = async (): Promise<void> => {
    setDesktopSourcesLoading(true);
    try {
      const sources = await getDesktopSources();
      setDesktopSources(sources);
    } catch (err) {
      console.error("Failed to load desktop sources:", err);
    } finally {
      setDesktopSourcesLoading(false);
    }
  };

  // Sync when panel opens
  createEffect(() => {
    if (isSettingsPanelOpen()) {
      setLocalSettings(getSettings());
      setDirty(false);
      void loadPlugins();
      void refreshDesktopSources();
    }
  });

  // Escape to close
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && isSettingsPanelOpen()) {
      closeSettingsPanel();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
  });

  const updateLocal = <K extends keyof AppSettings>(
    key: K,
    value: AppSettings[K]
  ) => {
    setLocalSettings((prev) => ({ ...prev, [key]: value }));
    setDirty(true);
  };

  const pluginTypeLabelKey = (pt: PluginType): TranslationKey => {
    const map: Record<PluginType, TranslationKey> = {
      theme: "pluginTypeTheme",
      widget: "pluginTypeWidget",
      organizer: "pluginTypeOrganizer",
    };
    return map[pt];
  };

  const handleInstallPlugin = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "BentoDesk Plugin", extensions: ["bdplugin"] }],
    });
    if (selected) {
      await installPluginAction(selected as string);
    }
  };

  const handleUninstallPlugin = async (id: string) => {
    await uninstallPluginAction(id);
    setConfirmingUninstall(null);
  };

  const handleSave = async () => {
    const current = localSettings();
    const updates: SettingsUpdate = {
      ghost_layer_enabled: current.ghost_layer_enabled,
      expand_delay_ms: current.expand_delay_ms,
      collapse_delay_ms: current.collapse_delay_ms,
      icon_cache_size: current.icon_cache_size,
      auto_group_enabled: current.auto_group_enabled,
      theme: current.theme,
      accent_color: current.accent_color,
      desktop_path: current.desktop_path,
      watch_paths: current.watch_paths,
      portable_mode: current.portable_mode,
      launch_at_startup: current.launch_at_startup,
      show_in_taskbar: current.show_in_taskbar,
      startup_high_priority: current.startup_high_priority,
      crash_restart_enabled: current.crash_restart_enabled,
      crash_max_retries: current.crash_max_retries,
      crash_window_secs: current.crash_window_secs,
      safe_start_after_hibernation: current.safe_start_after_hibernation,
      hibernate_resume_delay_ms: current.hibernate_resume_delay_ms,
    };
    const result = await updateSettingsStore(updates);
    if (result === null) {
      // Update failed — keep panel open so user sees the error / can retry
      return;
    }
    setDirty(false);
    closeSettingsPanel();
  };

  const handleCancel = () => {
    closeSettingsPanel();
  };

  return (
    <Show when={isSettingsPanelOpen()}>
      <div class="settings-overlay" onClick={handleCancel}>
        <div
          class="settings-panel scale-in"
          onClick={(e) => e.stopPropagation()}
        >
          <div class="settings-panel__header">
            <h2 class="settings-panel__title">{t("settingsTitle")}</h2>
            <button
              class="settings-panel__close"
              onClick={handleCancel}
              aria-label={t("settingsCloseAriaLabel")}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>

          <div class="settings-panel__body">
            {/* General */}
            <section class="settings-group">
              <h3 class="settings-group__title">{t("settingsGroupGeneral")}</h3>

              <ToggleRow
                label={t("settingsGhostLayer")}
                checked={localSettings().ghost_layer_enabled}
                onChange={(v) => updateLocal("ghost_layer_enabled", v)}
              />
              <ToggleRow
                label={t("settingsLaunchAtStartup")}
                checked={localSettings().launch_at_startup}
                onChange={(v) => updateLocal("launch_at_startup", v)}
              />
              <ToggleRow
                label={t("settingsShowInTaskbar")}
                checked={localSettings().show_in_taskbar}
                onChange={(v) => updateLocal("show_in_taskbar", v)}
              />
              <ToggleRow
                label={t("settingsAutoGroup")}
                checked={localSettings().auto_group_enabled}
                onChange={(v) => updateLocal("auto_group_enabled", v)}
              />
              <ToggleRow
                label={`${t("settingsPortableMode")} ${t("settingsPortableModeNote")}`}
                checked={localSettings().portable_mode}
                onChange={(v) => updateLocal("portable_mode", v)}
              />

              {/* Language selector */}
              <div class="settings-row">
                <span class="settings-row__label">{t("settingsLanguage")}</span>
                <select
                  class="settings-row__select"
                  value={getLocale()}
                  onChange={(e) =>
                    setLocale(e.currentTarget.value as Locale)
                  }
                >
                  <option value="zh-CN">{t("languageChinese")}</option>
                  <option value="en">{t("languageEnglish")}</option>
                </select>
              </div>
            </section>

            {/* Paths */}
            <section class="settings-group">
              <h3 class="settings-group__title">{t("settingsGroupPaths")}</h3>

              <div class="settings-row settings-row--column">
                <span class="settings-row__label">{t("settingsDesktopSources")}</span>
                <div class="desktop-source-list">
                  <Show
                    when={desktopSources().length > 0}
                    fallback={
                      <div class="desktop-source-empty">
                        {desktopSourcesLoading() ? "…" : t("settingsDesktopPathPlaceholder")}
                      </div>
                    }
                  >
                    <For each={desktopSources()}>
                      {(src) => (
                        <div class={`desktop-source-card desktop-source-card--${src.kind}`}>
                          <div class="desktop-source-card__icon" aria-hidden="true">
                            {desktopSourceInitial(src.kind)}
                          </div>
                          <div class="desktop-source-card__body">
                            <div class="desktop-source-card__label">
                              {t(desktopSourceLabelKey(src.kind))}
                            </div>
                            <div class="desktop-source-card__path" title={src.path}>
                              {src.path}
                            </div>
                          </div>
                          <Show when={src.watched}>
                            <span class="desktop-source-card__badge">
                              {t("desktopSourceWatched")}
                            </span>
                          </Show>
                        </div>
                      )}
                    </For>
                  </Show>
                  <button
                    class="settings-btn settings-btn--secondary desktop-source-refresh"
                    onClick={() => void refreshDesktopSources()}
                    disabled={desktopSourcesLoading()}
                  >
                    {desktopSourcesLoading() ? "…" : "↻"}
                  </button>
                </div>
              </div>

              <div class="settings-row settings-row--column">
                <span class="settings-row__label">{t("settingsDesktopPath")}</span>
                <input
                  class="settings-row__input"
                  type="text"
                  value={localSettings().desktop_path}
                  onInput={(e) =>
                    updateLocal("desktop_path", e.currentTarget.value)
                  }
                  placeholder={t("settingsDesktopPathPlaceholder")}
                />
              </div>

              <div class="settings-row settings-row--column">
                <span class="settings-row__label">{t("settingsWatchPaths")}</span>
                <textarea
                  class="settings-row__textarea"
                  value={localSettings().watch_paths.join("\n")}
                  onInput={(e) => {
                    const lines = e.currentTarget.value
                      .split("\n")
                      .map((l) => l.trim())
                      .filter((l) => l.length > 0);
                    updateLocal("watch_paths", lines);
                  }}
                  rows={3}
                  placeholder={t("settingsWatchPathsPlaceholder")}
                />
              </div>
            </section>

            {/* Appearance */}
            <section class="settings-group">
              <h3 class="settings-group__title">{t("settingsGroupAppearance")}</h3>

              <div class="settings-row settings-row--column">
                <span class="settings-row__label">{t("themePickerLabel")}</span>
                <div class="theme-groups" role="radiogroup" aria-label={t("themePickerLabel")}>
                  <For each={groupThemes(getAvailableThemes())}>
                    {(group) => (
                      <div class="theme-group">
                        <div class="theme-group__title">{t(group.key)}</div>
                        <div class="theme-grid">
                          <For each={group.themes}>
                            {(theme) => (
                              <ThemeCard
                                theme={theme}
                                active={getThemeId() === theme.id}
                                onSelect={() => {
                                  setTheme(theme.id);
                                  setDirty(true);
                                }}
                              />
                            )}
                          </For>
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </div>

              <div class="settings-row">
                <span class="settings-row__label">{t("settingsAccentColor")}</span>
                <input
                  class="settings-row__color"
                  type="color"
                  value={localSettings().accent_color}
                  onInput={(e) =>
                    updateLocal("accent_color", e.currentTarget.value)
                  }
                />
              </div>

              {/* Custom Theme — Developer Section */}
              <div class="settings-dev-section">
                <button
                  class="settings-dev-section__toggle"
                  onClick={() => setDevOpen((prev) => !prev)}
                  aria-expanded={devOpen()}
                  aria-label={t("developerOptions")}
                >
                  <svg
                    class={`settings-dev-section__chevron ${devOpen() ? "settings-dev-section__chevron--open" : ""}`}
                    width="12"
                    height="12"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <polyline points="9 18 15 12 9 6" />
                  </svg>
                  <span>{t("developerOptions")}</span>
                </button>

                <Show when={devOpen()}>
                  <div class="settings-dev-section__body">
                    <span class="settings-row__label">{t("customTheme")}</span>
                    <textarea
                      class="settings-row__textarea"
                      rows={6}
                      value={themeJson()}
                      onInput={(e) => {
                        setThemeJson(e.currentTarget.value);
                        setThemeFeedback(null);
                      }}
                      placeholder={t("themeJsonPlaceholder")}
                    />

                    <Show when={themeFeedback() !== null}>
                      <div
                        class={`settings-dev-section__feedback ${
                          themeFeedback()!.type === "success"
                            ? "settings-dev-section__feedback--success"
                            : "settings-dev-section__feedback--error"
                        }`}
                      >
                        {themeFeedback()!.message}
                      </div>
                    </Show>

                    <div class="settings-dev-section__actions">
                      <button
                        class="settings-btn settings-btn--secondary"
                        onClick={() => {
                          const json = themeJson().trim();
                          if (!json) {
                            setThemeFeedback({
                              type: "error",
                              message: t("themeImportError"),
                            });
                            return;
                          }
                          const result = importThemeFromJSON(json);
                          if (result) {
                            setThemeFeedback({
                              type: "success",
                              message: t("themeImportSuccess"),
                            });
                            setThemeJson("");
                            setTheme(result.id);
                          } else {
                            setThemeFeedback({
                              type: "error",
                              message: t("themeImportError"),
                            });
                          }
                        }}
                      >
                        {t("importTheme")}
                      </button>
                      <button
                        class="settings-btn settings-btn--secondary"
                        onClick={() => {
                          const json = exportThemeAsJSON(getThemeId());
                          void navigator.clipboard.writeText(json).then(() => {
                            setThemeFeedback({
                              type: "success",
                              message: t("exportTheme"),
                            });
                          });
                        }}
                      >
                        {t("exportTheme")}
                      </button>
                    </div>
                  </div>
                </Show>
              </div>
            </section>

            {/* Performance */}
            <section class="settings-group">
              <h3 class="settings-group__title">{t("settingsGroupPerformance")}</h3>

              <SliderRow
                label={t("settingsExpandDelay")}
                value={localSettings().expand_delay_ms}
                min={50}
                max={500}
                step={10}
                unit="ms"
                onChange={(v) => updateLocal("expand_delay_ms", v)}
              />
              <SliderRow
                label={t("settingsCollapseDelay")}
                value={localSettings().collapse_delay_ms}
                min={100}
                max={1000}
                step={50}
                unit="ms"
                onChange={(v) => updateLocal("collapse_delay_ms", v)}
              />
              <SliderRow
                label={t("settingsIconCacheSize")}
                value={localSettings().icon_cache_size}
                min={100}
                max={2000}
                step={100}
                unit=""
                onChange={(v) => updateLocal("icon_cache_size", v)}
              />
            </section>

            {/* Startup Management */}
            <section class="settings-group">
              <h3 class="settings-group__title">{t("settingsGroupStartup")}</h3>

              <ToggleRow
                label={t("settingsStartupHighPriority")}
                checked={localSettings().startup_high_priority}
                onChange={(v) => updateLocal("startup_high_priority", v)}
              />
              <div class="settings-row__desc">{t("settingsStartupHighPriorityDesc")}</div>

              <ToggleRow
                label={t("settingsCrashRestart")}
                checked={localSettings().crash_restart_enabled}
                onChange={(v) => updateLocal("crash_restart_enabled", v)}
              />
              <div class="settings-row__desc">{t("settingsCrashRestartDesc")}</div>

              <Show when={localSettings().crash_restart_enabled}>
                <div class="settings-row">
                  <span class="settings-row__label">{t("settingsCrashMaxRetries")}</span>
                  <input
                    class="settings-row__number-input"
                    type="number"
                    min={1}
                    max={10}
                    value={localSettings().crash_max_retries}
                    onInput={(e) =>
                      updateLocal("crash_max_retries", parseInt(e.currentTarget.value, 10) || 3)
                    }
                  />
                </div>
                <div class="settings-row">
                  <span class="settings-row__label">{t("settingsCrashWindowSecs")}</span>
                  <input
                    class="settings-row__number-input"
                    type="number"
                    min={5}
                    max={60}
                    value={localSettings().crash_window_secs}
                    onInput={(e) =>
                      updateLocal("crash_window_secs", parseInt(e.currentTarget.value, 10) || 10)
                    }
                  />
                </div>
              </Show>

              <ToggleRow
                label={t("settingsSafeStartHibernation")}
                checked={localSettings().safe_start_after_hibernation}
                onChange={(v) => updateLocal("safe_start_after_hibernation", v)}
              />
              <div class="settings-row__desc">{t("settingsSafeStartHibernationDesc")}</div>

              <Show when={localSettings().safe_start_after_hibernation}>
                <SliderRow
                  label={t("settingsHibernateDelay")}
                  value={localSettings().hibernate_resume_delay_ms}
                  min={500}
                  max={5000}
                  step={100}
                  unit="ms"
                  onChange={(v) => updateLocal("hibernate_resume_delay_ms", v)}
                />
              </Show>
            </section>

            {/* Plugins */}
            <section class="settings-group">
              <h3 class="settings-group__title">{t("settingsGroupPlugins")}</h3>

              <button
                class="settings-btn settings-btn--secondary plugin-install-btn"
                onClick={() => void handleInstallPlugin()}
              >
                {t("pluginInstall")}
              </button>

              <Show when={pluginsLoading()}>
                <div class="plugin-loading">{t("pluginEmpty")}</div>
              </Show>

              <Show when={!pluginsLoading() && plugins().length === 0}>
                <div class="plugin-empty">{t("pluginEmpty")}</div>
              </Show>

              <div class="plugin-list">
                <For each={plugins()}>
                  {(plugin: InstalledPlugin) => (
                    <div class="plugin-card">
                      <div class="plugin-card__header">
                        <div class="plugin-card__info">
                          <span class="plugin-card__name">{plugin.name}</span>
                          <span class="plugin-card__version">v{plugin.version}</span>
                          <span class={`plugin-card__badge plugin-card__badge--${plugin.plugin_type}`}>
                            {t(pluginTypeLabelKey(plugin.plugin_type))}
                          </span>
                        </div>
                        <button
                          class={`toggle-switch ${plugin.enabled ? "toggle-switch--on" : ""}`}
                          onClick={() => void togglePluginAction(plugin.id, !plugin.enabled)}
                          role="switch"
                          aria-checked={plugin.enabled}
                          aria-label={plugin.enabled ? t("pluginDisable") : t("pluginEnable")}
                        >
                          <div class="toggle-switch__thumb" />
                        </button>
                      </div>
                      <div class="plugin-card__author">{plugin.author}</div>
                      <div class="plugin-card__desc">{plugin.description}</div>
                      <div class="plugin-card__actions">
                        <Show when={confirmingUninstall() === plugin.id} fallback={
                          <button
                            class="settings-btn settings-btn--danger"
                            onClick={() => setConfirmingUninstall(plugin.id)}
                          >
                            {t("pluginUninstall")}
                          </button>
                        }>
                          <span class="plugin-card__confirm-text">
                            {t("pluginUninstallConfirm").replace("{name}", plugin.name)}
                          </span>
                          <button
                            class="settings-btn settings-btn--danger"
                            onClick={() => void handleUninstallPlugin(plugin.id)}
                          >
                            {t("pluginUninstall")}
                          </button>
                          <button
                            class="settings-btn settings-btn--secondary"
                            onClick={() => setConfirmingUninstall(null)}
                          >
                            {t("settingsBtnCancel")}
                          </button>
                        </Show>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </section>
          </div>

          <div class="settings-panel__footer">
            <button
              class="settings-btn settings-btn--secondary"
              onClick={handleCancel}
            >
              {t("settingsBtnCancel")}
            </button>
            <button
              class="settings-btn settings-btn--primary"
              onClick={() => void handleSave()}
              disabled={!dirty()}
            >
              {t("settingsBtnSave")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

// ─── Sub-components ──────────────────────────────────────────

interface ToggleRowProps {
  label: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}

const ToggleRow: Component<ToggleRowProps> = (props) => {
  return (
    <div class="settings-row">
      <span class="settings-row__label">{props.label}</span>
      <button
        class={`toggle-switch ${props.checked ? "toggle-switch--on" : ""}`}
        onClick={() => props.onChange(!props.checked)}
        role="switch"
        aria-checked={props.checked}
        aria-label={props.label}
      >
        <div class="toggle-switch__thumb" />
      </button>
    </div>
  );
};

interface SliderRowProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit: string;
  onChange: (value: number) => void;
}

const SliderRow: Component<SliderRowProps> = (props) => {
  return (
    <div class="settings-row settings-row--column">
      <div class="settings-row__top">
        <span class="settings-row__label">{props.label}</span>
        <span class="settings-row__value">
          {props.value}
          {props.unit}
        </span>
      </div>
      <input
        class="settings-slider"
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onInput={(e) =>
          props.onChange(parseInt(e.currentTarget.value, 10))
        }
      />
    </div>
  );
};

// ─── Theme Card ─────────────────────────────────────────────

interface ThemeCardProps {
  theme: BentoTheme;
  active: boolean;
  onSelect: () => void;
}

const ThemeCard: Component<ThemeCardProps> = (props) => {
  const themeName = (): string => {
    const key = props.theme.name_key as TranslationKey;
    return t(key);
  };

  return (
    <button
      class={`theme-card ${props.active ? "theme-card--active" : ""}`}
      onClick={props.onSelect}
      role="radio"
      aria-checked={props.active}
      aria-label={themeName()}
    >
      <div class="theme-card__swatches">
        <For each={props.theme.preview_colors}>
          {(color) => (
            <div
              class="theme-card__swatch"
              style={{ background: color }}
            />
          )}
        </For>
      </div>
      <span class="theme-card__label">{themeName()}</span>
    </button>
  );
};

export default SettingsPanel;

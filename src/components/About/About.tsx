/**
 * About — Simple modal dialog showing app info.
 * Displays: app name, version, description, license, and Tauri/webview info.
 */
import {
  Component,
  Show,
  createSignal,
  createEffect,
  onMount,
  onCleanup,
} from "solid-js";
import { isAboutDialogOpen, closeAboutDialog } from "../../stores/ui";
import { getSystemInfo } from "../../services/ipc";
import { t } from "../../i18n";
import type { SystemInfo } from "../../types/system";
import "./About.css";

const About: Component = () => {
  const [systemInfo, setSystemInfo] = createSignal<SystemInfo | null>(null);

  // Fetch system info when dialog opens
  createEffect(() => {
    if (isAboutDialogOpen()) {
      getSystemInfo()
        .then((info) => setSystemInfo(info))
        .catch((err) => console.error("Failed to get system info:", err));
    }
  });

  // Escape to close
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && isAboutDialogOpen()) {
      closeAboutDialog();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
  });

  return (
    <Show when={isAboutDialogOpen()}>
      <div class="about-overlay" onClick={() => closeAboutDialog()}>
        <div
          class="about-dialog scale-in"
          onClick={(e) => e.stopPropagation()}
        >
          <div class="about-dialog__icon">
            <img
              src="/bentodesk.svg"
              alt="BentoDesk"
              class="about-dialog__logo"
              width={64}
              height={64}
              draggable={false}
            />
          </div>
          <h2 class="about-dialog__name">{t("aboutAppName")}</h2>
          <p class="about-dialog__version">{t("aboutVersion")}</p>
          <p class="about-dialog__desc">
            {t("aboutDescription")}
          </p>

          <Show when={systemInfo()}>
            <div class="about-dialog__system">
              <div class="about-dialog__row">
                <span class="about-dialog__row-label">{t("aboutOs")}</span>
                <span class="about-dialog__row-value">
                  {systemInfo()!.os_version}
                </span>
              </div>
              <div class="about-dialog__row">
                <span class="about-dialog__row-label">{t("aboutDisplay")}</span>
                <span class="about-dialog__row-value">
                  {systemInfo()!.resolution.width}x{systemInfo()!.resolution.height}
                  {" @ "}
                  {systemInfo()!.dpi}dpi
                </span>
              </div>
              <Show when={systemInfo()!.webview2_version}>
                <div class="about-dialog__row">
                  <span class="about-dialog__row-label">{t("aboutWebView2")}</span>
                  <span class="about-dialog__row-value">
                    {systemInfo()!.webview2_version}
                  </span>
                </div>
              </Show>
            </div>
          </Show>

          <p class="about-dialog__license">
            {t("aboutLicense")}
          </p>

          <button
            class="about-dialog__close-btn"
            onClick={() => closeAboutDialog()}
          >
            {t("aboutClose")}
          </button>
        </div>
      </div>
    </Show>
  );
};

export default About;

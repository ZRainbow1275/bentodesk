/**
 * ContextMenu — Glassmorphism context menu that appears on right-click.
 * Positioned at cursor coordinates. Different menu items based on target type.
 * Closes on click outside, Escape, or menu item click.
 */
import {
  Component,
  For,
  Show,
  onMount,
  onCleanup,
  createMemo,
  createSignal,
} from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  getContextMenu,
  hideContextMenu,
  setConfirmDialogOpen,
} from "../../stores/ui";
import * as ipc from "../../services/ipc";
import * as zonesStore from "../../stores/zones";
import {
  expandZone,
  openSearch,
  openZoneEditor,
  openSmartGroupDialog,
  openBulkManager,
} from "../../stores/ui";
import { t } from "../../i18n";
import ZoneIcon from "../Icons/ZoneIcon";
import PromptModal from "../shared/PromptModal";
import "./ContextMenu.css";

interface MenuItem {
  icon: string;
  label: string;
  action: () => void;
  separator?: boolean;
  children?: MenuItem[];
  danger?: boolean;
  /** When true, clicking this item will NOT auto-close the context menu (e.g. to keep hit-test disabled while a confirm dialog is open). */
  keepMenuOpen?: boolean;
}

/** Confirmation dialog state */
interface ConfirmState {
  message: string;
  action: () => void;
}

/** Prompt dialog state (replaces native `window.prompt`). */
interface PromptState {
  title: string;
  defaultValue: string;
  placeholder?: string;
  onSubmit: (value: string) => void;
}

const ContextMenu: Component = () => {
  let menuRef: HTMLDivElement | undefined;
  let confirmOverlayRef: HTMLDivElement | undefined;
  const [hoveredSubmenu, setHoveredSubmenu] = createSignal<string | null>(null);
  const [confirm, setConfirmRaw] = createSignal<ConfirmState | null>(null);
  const [prompt, setPromptRaw] = createSignal<PromptState | null>(null);

  /** Wrapper that syncs confirm dialog state with ui store for hit-test awareness. */
  const setConfirm = (state: ConfirmState | null) => {
    setConfirmRaw(state);
    setConfirmDialogOpen(state !== null);
  };

  /** Wrapper that piggybacks on the confirm-dialog modal lock for hit-test awareness. */
  const setPrompt = (state: PromptState | null) => {
    setPromptRaw(state);
    setConfirmDialogOpen(state !== null);
  };

  const menu = () => getContextMenu();

  const buildMenuItems = createMemo((): MenuItem[] => {
    const m = menu();
    if (!m || !m.target) return [];

    switch (m.target.type) {
      case "zone":
        return buildZoneMenuItems(m.target, setPrompt);
      case "item":
        return buildItemMenuItems(m.target, setConfirm);
      default:
        return [];
    }
  });

  // Close on click outside
  const handleClickOutside = (e: MouseEvent) => {
    const target = e.target as Node;

    // If the prompt dialog is open, let PromptModal's backdrop click handler
    // manage dismissal — we must not auto-hide the context menu here.
    if (prompt()) {
      return;
    }

    // If the confirm dialog is open, only close it when clicking outside the
    // confirm overlay. Clicks inside the dialog (e.g. the Delete button) must
    // NOT trigger dismissal — the dialog's own handlers manage those.
    if (confirm()) {
      if (confirmOverlayRef && !confirmOverlayRef.contains(target)) {
        setConfirm(null);
        hideContextMenu();
      }
      return;
    }

    if (menuRef && !menuRef.contains(target)) {
      hideContextMenu();
      setHoveredSubmenu(null);
    }
  };

  // Close on Escape
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      // Prompt dialog has its own Escape handler; let it manage dismissal
      // and swallow this event so we don't also close the context menu.
      if (prompt()) return;
      if (confirm()) {
        setConfirm(null);
        hideContextMenu();
      } else {
        hideContextMenu();
        setHoveredSubmenu(null);
      }
    }
  };

  onMount(() => {
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("mousedown", handleClickOutside);
    document.removeEventListener("keydown", handleKeyDown);
  });

  // Adjust position to stay within viewport
  const menuPosition = () => {
    const m = menu();
    if (!m) return { left: "0px", top: "0px" };

    const x = Math.min(m.x, window.innerWidth - 220);
    const y = Math.min(m.y, window.innerHeight - 300);

    return {
      left: `${Math.max(0, x)}px`,
      top: `${Math.max(0, y)}px`,
    };
  };

  return (
    <>
      <Show when={menu()}>
        <div
          ref={menuRef}
          class="context-menu scale-in"
          style={{
            ...menuPosition(),
            position: "fixed",
            "z-index": "1000",
            "pointer-events": "auto",
          }}
        >
          <For each={buildMenuItems()}>
            {(item) => (
              <>
                <Show when={item.separator}>
                  <div class="context-menu__separator" />
                </Show>
                <Show
                  when={item.children && item.children.length > 0}
                  fallback={
                    <button
                      class={`context-menu__item ${item.danger ? "context-menu__item--danger" : ""}`}
                      onClick={() => {
                        item.action();
                        if (!item.keepMenuOpen) {
                          hideContextMenu();
                        }
                        setHoveredSubmenu(null);
                      }}
                    >
                      <span class="context-menu__item-icon">
                        <ZoneIcon icon={item.icon} size={16} />
                      </span>
                      <span class="context-menu__item-label">{item.label}</span>
                    </button>
                  }
                >
                  <div
                    class="context-menu__item context-menu__item--submenu"
                    onMouseEnter={() => setHoveredSubmenu(item.label)}
                    onMouseLeave={() => {
                      // Delay close so cursor can reach the submenu
                      const label = item.label;
                      setTimeout(() => {
                        if (hoveredSubmenu() === label) {
                          setHoveredSubmenu(null);
                        }
                      }, 200);
                    }}
                  >
                    <span class="context-menu__item-icon">
                      <ZoneIcon icon={item.icon} size={16} />
                    </span>
                    <span class="context-menu__item-label">{item.label}</span>
                    <span class="context-menu__item-arrow">{"\u{25B6}"}</span>
                    <Show when={hoveredSubmenu() === item.label}>
                      <div
                        class="context-menu__submenu scale-in"
                        onMouseEnter={() => setHoveredSubmenu(item.label)}
                        onMouseLeave={() => setHoveredSubmenu(null)}
                      >
                        <For each={item.children!}>
                          {(child) => (
                            <button
                              class="context-menu__item"
                              onClick={(e) => {
                                e.stopPropagation();
                                child.action();
                                hideContextMenu();
                                setHoveredSubmenu(null);
                              }}
                            >
                              <span class="context-menu__item-icon">
                                <ZoneIcon icon={child.icon} size={16} />
                              </span>
                              <span class="context-menu__item-label">{child.label}</span>
                            </button>
                          )}
                        </For>
                      </div>
                    </Show>
                  </div>
                </Show>
              </>
            )}
          </For>
        </div>
      </Show>

      {/* Text input dialog (replaces native window.prompt) */}
      <PromptModal
        open={prompt() !== null}
        title={prompt()?.title ?? ""}
        defaultValue={prompt()?.defaultValue ?? ""}
        placeholder={prompt()?.placeholder}
        okLabel={t("contextMenuBtnOk") || "OK"}
        cancelLabel={t("contextMenuBtnCancel") || "Cancel"}
        onSubmit={(value) => {
          const p = prompt();
          setPrompt(null);
          hideContextMenu();
          p?.onSubmit(value);
        }}
        onCancel={() => {
          setPrompt(null);
          hideContextMenu();
        }}
      />

      {/* Delete / destructive action confirmation dialog */}
      <Show when={confirm()}>
        {(confirmState) => (
          <div ref={confirmOverlayRef} class="confirm-overlay" onClick={() => { setConfirm(null); hideContextMenu(); }}>
            <div
              class="confirm-dialog scale-in"
              onClick={(e) => e.stopPropagation()}
            >
              <p class="confirm-dialog__message">{confirmState().message}</p>
              <div class="confirm-dialog__actions">
                <button
                  class="settings-btn settings-btn--secondary"
                  onClick={() => {
                    setConfirm(null);
                    hideContextMenu();
                  }}
                >
                  {t("contextMenuBtnCancel")}
                </button>
                <button
                  class="settings-btn settings-btn--danger"
                  onClick={() => {
                    confirmState().action();
                    setConfirm(null);
                    hideContextMenu();
                  }}
                >
                  {t("contextMenuBtnDelete")}
                </button>
              </div>
            </div>
          </div>
        )}
      </Show>
    </>
  );
};

// ─── Menu builders ───────────────────────────────────────────

function buildZoneMenuItems(
  target: { type: "zone"; zoneId: string },
  setPrompt: (state: PromptState | null) => void
): MenuItem[] {
  return [
    {
      icon: "edit",
      label: t("contextMenuEditZone"),
      action: () => {
        openZoneEditor(target.zoneId);
      },
    },
    {
      icon: "edit",
      label: t("contextMenuSetAlias"),
      action: () => {
        const zone = zonesStore.getZoneById(target.zoneId);
        if (!zone) return;
        setPrompt({
          title: t("contextMenuSetAliasPrompt"),
          defaultValue: zone.alias ?? "",
          onSubmit: (input) => {
            const trimmed = input.trim();
            void ipc
              .setZoneAlias(target.zoneId, trimmed === "" ? null : trimmed)
              .then(() => zonesStore.loadZones());
          },
        });
      },
      keepMenuOpen: true,
    },
    {
      icon: "grid",
      label: t("contextMenuAutoArrange"),
      action: () => {
        // Reorder items alphabetically via backend
        const zone = zonesStore.getZoneById(target.zoneId);
        if (zone) {
          const sorted = [...zone.items].sort((a, b) =>
            a.name.localeCompare(b.name, undefined, { sensitivity: "base" })
          );
          void zonesStore.reorderItems(
            target.zoneId,
            sorted.map((i) => i.id)
          );
        }
      },
    },
    {
      icon: "lightning",
      label: t("contextMenuSmartGroup"),
      action: () => {
        openSmartGroupDialog(target.zoneId);
      },
    },
    {
      icon: "search",
      label: t("contextMenuSearchInZone"),
      action: () => {
        expandZone(target.zoneId);
        openSearch(target.zoneId);
      },
    },
    {
      icon: "camera",
      label: t("contextMenuSaveSnapshot"),
      action: () => {
        ipc
          .saveSnapshot(`${t("appSnapshotPrefix")} ${new Date().toLocaleDateString()}`)
          .then(() => {
            // Brief visual feedback — the zone border flashes
            const el = document.querySelector(`[data-zone-id="${target.zoneId}"]`);
            if (el) {
              el.classList.add("bento-zone--snapshot-flash");
              setTimeout(() => el.classList.remove("bento-zone--snapshot-flash"), 600);
            }
          })
          .catch((err: unknown) => {
            const message =
              err instanceof Error ? err.message : String(err);
            console.error("Snapshot save failed:", message);
          });
      },
      separator: true,
    },
    // Theme E2-c — Pin zone as a floating Mini Bar. Backend enforces a
    // hard cap of 3 active minibars because each spawns its own WebView2
    // process.
    {
      icon: "pin",
      label: t("contextMenuPinMinibar") || "Pin as Mini Bar",
      action: () => {
        void invoke("pin_zone_as_minibar", { zoneId: target.zoneId })
          .catch((e) => console.error("pin_zone_as_minibar failed:", e));
      },
    },
    // Theme E2-e — Bind zone to a folder. Once bound, the folder's
    // contents mirror into the zone (read-only, one-way).
    {
      icon: "folder",
      label: t("contextMenuBindFolder") || "Bind Folder…",
      action: () => {
        void (async () => {
          const selected = await openDialog({ directory: true, multiple: false });
          if (typeof selected !== "string") return;
          try {
            await invoke("bind_zone_to_folder", {
              zoneId: target.zoneId,
              folderPath: selected,
            });
            await zonesStore.loadZones();
          } catch (e) {
            console.error("bind_zone_to_folder failed:", e);
          }
        })();
      },
    },
    // Theme E2-a — Capture current foreground-window layout. The capsule
    // is app-scope (not zone-scope) but exposed here for discoverability.
    {
      icon: "camera",
      label: t("contextMenuSaveCapsule") || "Save as Context Capsule",
      action: () => {
        setPrompt({
          title: t("contextMenuSaveCapsulePrompt") || "Capsule name?",
          defaultValue: `Capsule ${new Date().toLocaleString()}`,
          onSubmit: (input) => {
            const name = input.trim();
            if (!name) return;
            void invoke("capture_context", { name })
              .catch((e) => console.error("capture_context failed:", e));
          },
        });
      },
      keepMenuOpen: true,
      separator: true,
    },
    {
      icon: "settings",
      label: t("bulkManagerContextEntry"),
      action: () => {
        openBulkManager();
      },
      separator: true,
    },
    {
      icon: "trash",
      label: t("contextMenuDeleteZone"),
      action: () => {
        void zonesStore.deleteZone(target.zoneId);
      },
      separator: true,
      danger: true,
    },
  ];
}

function buildItemMenuItems(
  target: { type: "item"; zoneId: string; itemId: string },
  setConfirm: (state: ConfirmState | null) => void
): MenuItem[] {
  const zone = zonesStore.getZoneById(target.zoneId);
  const item = zone?.items.find((i) => i.id === target.itemId);
  if (!item) return [];

  // Build "Move to Zone" as flat menu items (more reliable than hover submenu)
  const otherZones = zonesStore.getZones().filter((z) => z.id !== target.zoneId);

  const items: MenuItem[] = [
    {
      icon: "external-link",
      label: t("contextMenuOpenFile"),
      action: () => {
        void ipc.openFile(item.path);
      },
    },
    {
      icon: "folder-open",
      label: t("contextMenuRevealInExplorer"),
      action: () => {
        void ipc.revealInExplorer(item.path);
      },
    },
    {
      icon: "copy",
      label: t("contextMenuCopyPath"),
      action: () => {
        void navigator.clipboard.writeText(item.path);
      },
      separator: true,
    },
    {
      icon: item.is_wide ? "square" : "columns",
      label: item.is_wide ? t("contextMenuSetNormalCard") : t("contextMenuSetWideCard"),
      action: () => {
        void zonesStore.toggleItemWide(target.zoneId, target.itemId);
      },
    },
  ];

  // Add "Move to Zone" submenu — uses children array for nested rendering
  if (otherZones.length > 0) {
    const moveChildren: MenuItem[] = otherZones.map((z) => ({
      icon: "folder",
      label: z.name,
      action: () => {
        void zonesStore.moveItem(target.zoneId, z.id, target.itemId);
      },
    }));
    items.push({
      icon: "arrow-right",
      label: t("contextMenuMoveToZone"),
      action: () => {}, // Parent item — no direct action
      children: moveChildren,
    });
  }

  items.push({
    icon: "trash",
    label: t("contextMenuRemoveFromZone"),
    action: () => {
      const cleanName = item.name.replace(/\.(lnk|url)$/i, "");
      setConfirm({
        message: t("contextMenuConfirmRemove").replace("{name}", cleanName),
        action: () => {
          void zonesStore.removeItem(target.zoneId, target.itemId);
        },
      });
    },
    separator: true,
    danger: true,
    keepMenuOpen: true,
  });

  return items;
}

export default ContextMenu;

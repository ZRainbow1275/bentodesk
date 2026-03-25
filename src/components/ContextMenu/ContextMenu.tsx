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
} from "../../stores/ui";
import { t } from "../../i18n";
import ZoneIcon from "../Icons/ZoneIcon";
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

const ContextMenu: Component = () => {
  let menuRef: HTMLDivElement | undefined;
  let confirmOverlayRef: HTMLDivElement | undefined;
  const [hoveredSubmenu, setHoveredSubmenu] = createSignal<string | null>(null);
  const [confirm, setConfirmRaw] = createSignal<ConfirmState | null>(null);

  /** Wrapper that syncs confirm dialog state with ui store for hit-test awareness. */
  const setConfirm = (state: ConfirmState | null) => {
    setConfirmRaw(state);
    setConfirmDialogOpen(state !== null);
  };

  const menu = () => getContextMenu();

  const buildMenuItems = createMemo((): MenuItem[] => {
    const m = menu();
    if (!m || !m.target) return [];

    switch (m.target.type) {
      case "zone":
        return buildZoneMenuItems(m.target);
      case "item":
        return buildItemMenuItems(m.target, setConfirm);
      default:
        return [];
    }
  });

  // Close on click outside
  const handleClickOutside = (e: MouseEvent) => {
    const target = e.target as Node;

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

function buildZoneMenuItems(target: { type: "zone"; zoneId: string }): MenuItem[] {
  return [
    {
      icon: "edit",
      label: t("contextMenuEditZone"),
      action: () => {
        openZoneEditor(target.zoneId);
      },
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

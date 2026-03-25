/**
 * Keyboard shortcut manager for BentoDesk.
 * Handles Tab, Enter, Space, Arrow keys, Ctrl+F, Escape, Delete.
 */
import { isAnyModalOpen } from "../stores/ui";

export interface HotkeyHandlers {
  onTab: (e: KeyboardEvent) => void;
  onEnter: (e: KeyboardEvent) => void;
  onSpace: (e: KeyboardEvent) => void;
  onArrowUp: (e: KeyboardEvent) => void;
  onArrowDown: (e: KeyboardEvent) => void;
  onArrowLeft: (e: KeyboardEvent) => void;
  onArrowRight: (e: KeyboardEvent) => void;
  onDelete: (e: KeyboardEvent) => void;
  onCtrlF: (e: KeyboardEvent) => void;
  onEscape: (e: KeyboardEvent) => void;
}

let registeredHandlers: HotkeyHandlers | null = null;
let keydownListener: ((e: KeyboardEvent) => void) | null = null;

/**
 * Register global keyboard shortcut handlers.
 * Call once on app mount; returns a cleanup function.
 */
export function registerHotkeys(handlers: HotkeyHandlers): () => void {
  registeredHandlers = handlers;

  keydownListener = (e: KeyboardEvent) => {
    if (!registeredHandlers) return;

    // Ctrl+F: search
    if (e.ctrlKey && e.key === "f") {
      e.preventDefault();
      registeredHandlers.onCtrlF(e);
      return;
    }

    // Don't intercept if user is typing in an input
    const target = e.target as HTMLElement;
    if (
      target.tagName === "INPUT" ||
      target.tagName === "TEXTAREA" ||
      target.isContentEditable
    ) {
      // Only handle Escape in inputs
      if (e.key === "Escape") {
        registeredHandlers.onEscape(e);
      }
      return;
    }

    // When any modal/dialog is open, skip global hotkeys.
    // Escape is NOT handled here — each modal registers its own keydown
    // listener that handles Escape independently.
    if (isAnyModalOpen()) {
      return;
    }

    switch (e.key) {
      case "Tab":
        e.preventDefault();
        registeredHandlers.onTab(e);
        break;
      case "Enter":
        e.preventDefault();
        registeredHandlers.onEnter(e);
        break;
      case " ":
        e.preventDefault();
        registeredHandlers.onSpace(e);
        break;
      case "ArrowUp":
        e.preventDefault();
        registeredHandlers.onArrowUp(e);
        break;
      case "ArrowDown":
        e.preventDefault();
        registeredHandlers.onArrowDown(e);
        break;
      case "ArrowLeft":
        e.preventDefault();
        registeredHandlers.onArrowLeft(e);
        break;
      case "ArrowRight":
        e.preventDefault();
        registeredHandlers.onArrowRight(e);
        break;
      case "Delete":
        e.preventDefault();
        registeredHandlers.onDelete(e);
        break;
      case "Escape":
        registeredHandlers.onEscape(e);
        break;
    }
  };

  document.addEventListener("keydown", keydownListener);

  return () => {
    if (keydownListener) {
      document.removeEventListener("keydown", keydownListener);
      keydownListener = null;
    }
    registeredHandlers = null;
  };
}

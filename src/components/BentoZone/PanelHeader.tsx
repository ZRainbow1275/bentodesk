/**
 * PanelHeader — Header bar for expanded BentoZone.
 * Contains: zone icon, title, item count badge, search toggle, close button.
 */
import { Component, createSignal, createMemo, onMount, onCleanup } from "solid-js";
import type { BentoZone } from "../../types/zone";
import { openSearch, isSearchActive } from "../../stores/ui";
import { showContextMenu } from "../../stores/ui";
import { t } from "../../i18n";
import ZoneIcon from "../Icons/ZoneIcon";
import Tooltip from "../shared/Tooltip";
import { smartAbbreviate, getFontCtx } from "../../services/textAbbr";
import "./PanelHeader.css";

interface PanelHeaderProps {
  zone: BentoZone;
  onDragStart: (e: MouseEvent) => void;
  onClose: () => void;
}

const PanelHeader: Component<PanelHeaderProps> = (props) => {
  const handleSearchClick = () => {
    if (!isSearchActive(props.zone.id)) {
      openSearch(props.zone.id);
    }
  };

  const handleContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    showContextMenu(e.clientX, e.clientY, {
      type: "zone",
      zoneId: props.zone.id,
    });
  };

  const fullName = () => props.zone.alias ?? props.zone.name;

  const [titleEl, setTitleEl] = createSignal<HTMLElement | undefined>();
  const [maxPx, setMaxPx] = createSignal(0);
  const [fontCtx, setFontCtx] = createSignal<{ font: string }>({ font: "12px sans-serif" });

  onMount(() => {
    const el = titleEl();
    if (!el) return;
    setFontCtx(getFontCtx(el));
    const measure = () => setMaxPx(el.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    onCleanup(() => ro.disconnect());
  });

  const abbreviated = createMemo(() => {
    const w = maxPx();
    const name = fullName();
    if (w <= 0) return name;
    return smartAbbreviate(name, w, fontCtx());
  });

  const tooltipDisabled = createMemo(() => abbreviated() === fullName());

  return (
    <div
      class="panel-header"
      onMouseDown={props.onDragStart}
      onContextMenu={handleContextMenu}
    >
      <span class="panel-header__icon">
          <ZoneIcon icon={props.zone.icon} size={16} />
        </span>
      <span
        class="panel-header__title"
        ref={setTitleEl}
        aria-label={fullName()}
      >
        <Tooltip content={fullName()} disabled={tooltipDisabled()}>
          {abbreviated()}
        </Tooltip>
      </span>
      <span class="panel-header__badge">{props.zone.items.length}</span>
      <div class="panel-header__actions">
        <button
          class="panel-header__btn"
          onClick={(e) => {
            e.stopPropagation();
            handleSearchClick();
          }}
          onMouseDown={(e) => e.stopPropagation()}
          title={t("panelHeaderSearchTitle")}
          aria-label={t("panelHeaderSearchAriaLabel")}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
        </button>
        <button
          class="panel-header__btn panel-header__btn--close"
          onClick={(e) => {
            e.stopPropagation();
            props.onClose();
          }}
          onMouseDown={(e) => e.stopPropagation()}
          title={t("panelHeaderCloseTitle")}
          aria-label={t("panelHeaderCloseAriaLabel")}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </div>
  );
};

export default PanelHeader;

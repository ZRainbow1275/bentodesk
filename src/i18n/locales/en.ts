/**
 * English locale for BentoDesk.
 * Keys match zh-CN.ts exactly; values are English translations.
 */
import type { Translations } from "./zh-CN";

const en: Translations = {
  // ─── Settings Panel ────────────────────────────────────────
  settingsTitle: "Settings",
  settingsCloseAriaLabel: "Close settings",
  settingsGroupGeneral: "General",
  settingsGroupPaths: "Paths",
  settingsGroupAppearance: "Appearance",
  settingsGroupPerformance: "Performance",
  settingsGhostLayer: "Desktop Embed Layer",
  settingsLaunchAtStartup: "Launch at Startup",
  settingsShowInTaskbar: "Show in Taskbar",
  settingsAutoGroup: "Smart Auto Group",
  settingsPortableMode: "Portable Mode",
  settingsPortableModeNote: "(restart required)",
  settingsDesktopPath: "Desktop Path",
  settingsDesktopPathPlaceholder: "C:\\Users\\...\\Desktop",
  settingsWatchPaths: "Watch Paths (one per line)",
  settingsWatchPathsPlaceholder: "Add folders to watch...",
  settingsTheme: "Theme",
  settingsThemeDark: "Dark",
  settingsThemeLight: "Light",
  settingsThemeSystem: "Follow System",
  settingsAccentColor: "Accent Color",
  settingsExpandDelay: "Expand Delay",
  settingsCollapseDelay: "Collapse Delay",
  settingsIconCacheSize: "Icon Cache Size",
  settingsBtnCancel: "Cancel",
  settingsBtnSave: "Save",
  settingsLanguage: "Language",

  // ─── Context Menu ──────────────────────────────────────────
  contextMenuEditZone: "Edit Zone",
  contextMenuAutoArrange: "Auto Arrange Items",
  contextMenuSmartGroup: "Smart Group Suggestions",
  contextMenuSearchInZone: "Search in Zone",
  contextMenuSaveSnapshot: "Save Layout Snapshot",
  contextMenuDeleteZone: "Delete Zone",
  contextMenuOpenFile: "Open File",
  contextMenuRevealInExplorer: "Reveal in Explorer",
  contextMenuCopyPath: "Copy Path",
  contextMenuSetNormalCard: "Set as Normal Card",
  contextMenuSetWideCard: "Set as Wide Card",
  contextMenuMoveToZone: "Move to Zone",
  contextMenuRemoveFromZone: "Remove from Zone",
  contextMenuConfirmRemove: 'Are you sure you want to remove "{name}" from this zone?',
  contextMenuBtnCancel: "Cancel",
  contextMenuBtnDelete: "Delete",

  // ─── Zone Editor ───────────────────────────────────────────
  zoneEditorTitle: "Edit Zone",
  zoneEditorCloseAriaLabel: "Close editor",
  zoneEditorZoneName: "Zone Name",
  zoneEditorZoneNamePlaceholder: "Zone name",
  zoneEditorIcon: "Icon",
  zoneEditorAccentColor: "Accent Color",
  zoneEditorAccentColorNone: "None",
  zoneEditorGridColumns: "Grid Columns",
  zoneEditorCapsuleShape: "Capsule Shape",
  zoneEditorCapsuleShapePill: "Pill",
  zoneEditorCapsuleShapeRounded: "Rounded",
  zoneEditorCapsuleShapeCircle: "Circle",
  zoneEditorCapsuleShapeMinimal: "Minimal",
  zoneEditorCapsuleSize: "Capsule Size",
  zoneEditorCapsuleSizeSmall: "Small",
  zoneEditorCapsuleSizeMedium: "Medium",
  zoneEditorCapsuleSizeLarge: "Large",
  zoneEditorBtnCancel: "Cancel",
  zoneEditorBtnSave: "Save",

  // ─── Smart Group Suggestor ─────────────────────────────────
  smartGroupTitle: "Smart Group Suggestions",
  smartGroupCloseAriaLabel: "Close",
  smartGroupScanning: "Scanning desktop files...",
  smartGroupAnalyzing: "Analyzing file patterns...",
  smartGroupError: "Analysis failed: ",
  smartGroupEmpty: "No group suggestions found. Try adding more files to your desktop.",
  smartGroupFiles: "files",
  smartGroupConfidenceHigh: "High",
  smartGroupConfidenceMedium: "Medium",
  smartGroupConfidenceLow: "Low",
  smartGroupApplyToZone: "Apply to current zone",
  smartGroupApplying: "Applying...",
  smartGroupApply: "Apply",
  smartGroupNewZone: "+ New Zone",
  smartGroupCreateAsNewZone: "Create as new zone",

  // ─── Snapshot Picker ───────────────────────────────────────
  snapshotPickerTitle: "Layout Snapshots",
  snapshotPickerCloseAriaLabel: "Close",
  snapshotPickerLoading: "Loading snapshots...",
  snapshotPickerEmpty: 'No saved snapshots. Right-click a zone and select "Save Layout Snapshot" to create one.',
  snapshotPickerZones: "zones",
  snapshotPickerLoad: "Load",
  snapshotPickerDelete: "Delete",
  snapshotPickerConfirmDelete: "Confirm delete?",
  snapshotPickerConfirmYes: "Yes",
  snapshotPickerConfirmNo: "No",

  // ─── Search Bar ────────────────────────────────────────────
  searchBarPlaceholder: "Search items...",
  searchBarClearAriaLabel: "Clear search",

  // ─── Item Grid ─────────────────────────────────────────────
  itemGridEmptyDropHere: "Drop files here",

  // ─── About Dialog ──────────────────────────────────────────
  aboutAppName: "BentoDesk",
  aboutVersion: "v0.1.0",
  aboutDescription: "A bento-box style Windows desktop organizer. Frosted glass zones float above your wallpaper.",
  aboutOs: "Operating System",
  aboutDisplay: "Display",
  aboutWebView2: "WebView2",
  aboutLicense: "MIT License",
  aboutClose: "Close",

  // ─── App / Tray ────────────────────────────────────────────
  appNewZone: "New Zone",
  appNewZonePrefix: "New Zone",
  appAutoOrganize: "Auto Organize",
  appSnapshotPrefix: "Snapshot",

  // ─── Panel Header ──────────────────────────────────────────
  panelHeaderSearchTitle: "Search (Ctrl+F)",
  panelHeaderSearchAriaLabel: "Search",
  panelHeaderCloseTitle: "Close",
  panelHeaderCloseAriaLabel: "Close zone",

  // ─── Language Options ──────────────────────────────────────
  languageChinese: "中文",
  languageEnglish: "English",

  // ─── Theme Names ─────────────────────────────────────────
  themeDark: "Dark",
  themeLight: "Light",
  themeMidnight: "Midnight",
  themeForest: "Forest",
  themeSunset: "Sunset",
  themeFrosted: "Frosted",
  themeSolid: "Solid",
  themeOrder: "Order",
  themeNeo: "Neo",
  themeFlat: "Flat",
  themeCustom: "Custom",
  themePickerLabel: "Choose Theme",

  // ─── Custom Theme (Developer) ────────────────────────────
  customTheme: "Custom Theme",
  importTheme: "Import Theme",
  exportTheme: "Export Current Theme",
  themeImportSuccess: "Theme imported successfully",
  themeImportError: "Theme import failed",
  themeJsonPlaceholder: "Paste theme JSON...",
  developerOptions: "Developer Options",

  // ─── Startup Management ─────────────────────────────────────
  settingsGroupStartup: "Startup",
  settingsStartupHighPriority: "High Priority Startup",
  settingsStartupHighPriorityDesc: "Launch immediately after login, no delay",
  settingsCrashRestart: "Crash Auto Restart",
  settingsCrashRestartDesc: "Monitor crashes via Guardian and auto restart",
  settingsCrashMaxRetries: "Max Retries",
  settingsCrashWindowSecs: "Crash Window (sec)",
  settingsSafeStartHibernation: "Hibernate Safe Recovery",
  settingsSafeStartHibernationDesc: "Delay startup after hibernate to ensure stability",
  settingsHibernateDelay: "Resume Delay",
  settingsHibernateDelayMs: "{value}ms",

  // ─── Plugin Manager ─────────────────────────────────────────
  settingsGroupPlugins: "Plugins",
  pluginInstall: "Install Plugin...",
  pluginUninstall: "Uninstall",
  pluginUninstallConfirm: "Are you sure you want to uninstall {name}?",
  pluginEnable: "Enable",
  pluginDisable: "Disable",
  pluginTypeTheme: "Theme",
  pluginTypeWidget: "Widget",
  pluginTypeOrganizer: "Organizer",
  pluginEmpty: "No plugins installed",
  pluginVersion: "v{version}",
  pluginInstalledSuccess: "Plugin installed successfully",
  pluginUninstalledSuccess: "Plugin uninstalled",
};

export default en;

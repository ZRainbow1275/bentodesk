# Internationalization Guide

BentoDesk uses a lightweight i18n system built on Solid.js reactive signals. The current implementation supports **Chinese (Simplified)** and **English**, with an architecture designed for easy addition of new languages.

Source files:
- `src/i18n/index.ts` -- store, `t()` function, locale switching
- `src/i18n/locales/zh-CN.ts` -- Chinese translations (canonical key definitions)
- `src/i18n/locales/en.ts` -- English translations

## How `t()` Works

The `t()` function is the primary API for translating strings in components:

```typescript
import { t } from "../i18n";

// In a Solid.js component:
<span>{t("settingsTitle")}</span>
// Renders "Settings" (en) or "设置" (zh-CN) based on active locale
```

Because `t()` reads a Solid.js signal internally, it is **reactive** -- any component calling `t(key)` will automatically re-render when the locale changes. There is no need for explicit subscription or context providers.

### Type Safety

All translation keys are strongly typed via `TranslationKey`:

```typescript
export type TranslationKey = keyof typeof zhCN;
```

This means:
- TypeScript will error if you use a key that does not exist
- Autocomplete works for all available keys
- Adding a key to `zh-CN.ts` requires adding it to all other locale files (enforced by `Translations` type)

## Locale Persistence

The active locale is saved to `localStorage` under the key `bentodesk-locale`. On app startup, the saved locale is restored. If no saved locale exists, the default is `"zh-CN"`.

```typescript
// Switching locale
import { setLocale } from "../i18n";
setLocale("en");      // Switches to English, saves to localStorage
setLocale("zh-CN");   // Switches to Chinese, saves to localStorage

// Reading current locale (reactive)
import { getLocale } from "../i18n";
const locale = getLocale();  // "zh-CN" | "en"
```

## All Translation Keys

### Settings Panel

| Key | zh-CN | en |
|-----|-------|----|
| `settingsTitle` | 设置 | Settings |
| `settingsCloseAriaLabel` | 关闭设置 | Close settings |
| `settingsGroupGeneral` | 通用 | General |
| `settingsGroupPaths` | 路径 | Paths |
| `settingsGroupAppearance` | 外观 | Appearance |
| `settingsGroupPerformance` | 性能 | Performance |
| `settingsGhostLayer` | 桌面嵌入层 | Desktop Embed Layer |
| `settingsLaunchAtStartup` | 开机启动 | Launch at Startup |
| `settingsShowInTaskbar` | 显示在任务栏 | Show in Taskbar |
| `settingsAutoGroup` | 智能自动分组 | Smart Auto Group |
| `settingsPortableMode` | 便携模式 | Portable Mode |
| `settingsPortableModeNote` | (需要重启) | (restart required) |
| `settingsDesktopPath` | 桌面路径 | Desktop Path |
| `settingsDesktopPathPlaceholder` | C:\Users\...\Desktop | C:\Users\...\Desktop |
| `settingsWatchPaths` | 监控路径（每行一个） | Watch Paths (one per line) |
| `settingsWatchPathsPlaceholder` | 添加要监控的文件夹... | Add folders to watch... |
| `settingsTheme` | 主题 | Theme |
| `settingsThemeDark` | 深色 | Dark |
| `settingsThemeLight` | 浅色 | Light |
| `settingsThemeSystem` | 跟随系统 | Follow System |
| `settingsAccentColor` | 强调色 | Accent Color |
| `settingsExpandDelay` | 展开延迟 | Expand Delay |
| `settingsCollapseDelay` | 收起延迟 | Collapse Delay |
| `settingsIconCacheSize` | 图标缓存大小 | Icon Cache Size |
| `settingsBtnCancel` | 取消 | Cancel |
| `settingsBtnSave` | 保存 | Save |
| `settingsLanguage` | 语言 / Language | Language |

### Context Menu

| Key | zh-CN | en |
|-----|-------|----|
| `contextMenuEditZone` | 编辑区域 | Edit Zone |
| `contextMenuAutoArrange` | 自动排列项目 | Auto Arrange Items |
| `contextMenuSmartGroup` | 智能分组建议 | Smart Group Suggestions |
| `contextMenuSearchInZone` | 在区域中搜索 | Search in Zone |
| `contextMenuSaveSnapshot` | 保存布局快照 | Save Layout Snapshot |
| `contextMenuDeleteZone` | 删除区域 | Delete Zone |
| `contextMenuOpenFile` | 打开文件 | Open File |
| `contextMenuRevealInExplorer` | 在资源管理器中显示 | Reveal in Explorer |
| `contextMenuCopyPath` | 复制路径 | Copy Path |
| `contextMenuSetNormalCard` | 设为普通卡片 | Set as Normal Card |
| `contextMenuSetWideCard` | 设为宽卡片 | Set as Wide Card |
| `contextMenuMoveToZone` | 移动到区域 | Move to Zone |
| `contextMenuRemoveFromZone` | 从区域中移除 | Remove from Zone |
| `contextMenuConfirmRemove` | 确定要从此区域中移除"{name}"吗？ | Are you sure you want to remove "{name}" from this zone? |
| `contextMenuBtnCancel` | 取消 | Cancel |
| `contextMenuBtnDelete` | 删除 | Delete |

### Zone Editor

| Key | zh-CN | en |
|-----|-------|----|
| `zoneEditorTitle` | 编辑区域 | Edit Zone |
| `zoneEditorCloseAriaLabel` | 关闭编辑器 | Close editor |
| `zoneEditorZoneName` | 区域名称 | Zone Name |
| `zoneEditorZoneNamePlaceholder` | 区域名称 | Zone name |
| `zoneEditorIcon` | 图标 | Icon |
| `zoneEditorAccentColor` | 强调色 | Accent Color |
| `zoneEditorAccentColorNone` | None | None |
| `zoneEditorGridColumns` | 网格列数 | Grid Columns |
| `zoneEditorCapsuleShape` | 胶囊形状 | Capsule Shape |
| `zoneEditorCapsuleShapePill` | 药丸 | Pill |
| `zoneEditorCapsuleShapeRounded` | 圆角 | Rounded |
| `zoneEditorCapsuleShapeCircle` | 圆形 | Circle |
| `zoneEditorCapsuleShapeMinimal` | 极简 | Minimal |
| `zoneEditorCapsuleSize` | 胶囊大小 | Capsule Size |
| `zoneEditorCapsuleSizeSmall` | 小 | Small |
| `zoneEditorCapsuleSizeMedium` | 中 | Medium |
| `zoneEditorCapsuleSizeLarge` | 大 | Large |
| `zoneEditorBtnCancel` | 取消 | Cancel |
| `zoneEditorBtnSave` | 保存 | Save |

### Smart Group Suggestor

| Key | zh-CN | en |
|-----|-------|----|
| `smartGroupTitle` | 智能分组建议 | Smart Group Suggestions |
| `smartGroupCloseAriaLabel` | 关闭 | Close |
| `smartGroupScanning` | 正在扫描桌面文件... | Scanning desktop files... |
| `smartGroupAnalyzing` | 正在分析文件模式... | Analyzing file patterns... |
| `smartGroupError` | 分析失败： | Analysis failed: |
| `smartGroupEmpty` | 未找到分组建议。请尝试在桌面添加更多文件。 | No group suggestions found. Try adding more files to your desktop. |
| `smartGroupFiles` | 个文件 | files |
| `smartGroupConfidenceHigh` | 高 | High |
| `smartGroupConfidenceMedium` | 中 | Medium |
| `smartGroupConfidenceLow` | 低 | Low |
| `smartGroupApplyToZone` | 应用到当前区域 | Apply to current zone |
| `smartGroupApplying` | 应用中... | Applying... |
| `smartGroupApply` | 应用 | Apply |
| `smartGroupNewZone` | + 新区域 | + New Zone |
| `smartGroupCreateAsNewZone` | 创建为新区域 | Create as new zone |

### Snapshot Picker

| Key | zh-CN | en |
|-----|-------|----|
| `snapshotPickerTitle` | 布局快照 | Layout Snapshots |
| `snapshotPickerCloseAriaLabel` | 关闭 | Close |
| `snapshotPickerLoading` | 正在加载快照... | Loading snapshots... |
| `snapshotPickerEmpty` | 暂无保存的快照。右键点击区域并选择"保存布局快照"来创建一个。 | No saved snapshots. Right-click a zone and select "Save Layout Snapshot" to create one. |
| `snapshotPickerZones` | 个区域 | zones |
| `snapshotPickerLoad` | 加载 | Load |
| `snapshotPickerDelete` | 删除 | Delete |
| `snapshotPickerConfirmDelete` | 确认删除？ | Confirm delete? |
| `snapshotPickerConfirmYes` | 是 | Yes |
| `snapshotPickerConfirmNo` | 否 | No |

### Search

| Key | zh-CN | en |
|-----|-------|----|
| `searchBarPlaceholder` | 搜索项目... | Search items... |
| `searchBarClearAriaLabel` | 清除搜索 | Clear search |

### Item Grid

| Key | zh-CN | en |
|-----|-------|----|
| `itemGridEmptyDropHere` | 拖放文件到这里 | Drop files here |

### About Dialog

| Key | zh-CN | en |
|-----|-------|----|
| `aboutAppName` | BentoDesk | BentoDesk |
| `aboutVersion` | v0.1.0 | v0.1.0 |
| `aboutDescription` | 一款便当盒风格的 Windows 桌面整理工具。毛玻璃区域悬浮于壁纸之上。 | A bento-box style Windows desktop organizer. Frosted glass zones float above your wallpaper. |
| `aboutOs` | 操作系统 | Operating System |
| `aboutDisplay` | 显示器 | Display |
| `aboutWebView2` | WebView2 | WebView2 |
| `aboutLicense` | MIT 许可证 | MIT License |
| `aboutClose` | 关闭 | Close |

### App / Tray

| Key | zh-CN | en |
|-----|-------|----|
| `appNewZone` | 新建区域 | New Zone |
| `appNewZonePrefix` | 新建区域 | New Zone |
| `appAutoOrganize` | 自动整理 | Auto Organize |
| `appSnapshotPrefix` | 快照 | Snapshot |

### Panel Header

| Key | zh-CN | en |
|-----|-------|----|
| `panelHeaderSearchTitle` | 搜索 (Ctrl+F) | Search (Ctrl+F) |
| `panelHeaderSearchAriaLabel` | 搜索 | Search |
| `panelHeaderCloseTitle` | 关闭 | Close |
| `panelHeaderCloseAriaLabel` | 关闭区域 | Close zone |

### Language Options

| Key | zh-CN | en |
|-----|-------|----|
| `languageChinese` | 中文 | 中文 |
| `languageEnglish` | English | English |

### Theme Names

| Key | zh-CN | en |
|-----|-------|----|
| `themeDark` | 深色 | Dark |
| `themeLight` | 浅色 | Light |
| `themeMidnight` | 午夜 | Midnight |
| `themeForest` | 森林 | Forest |
| `themeSunset` | 日落 | Sunset |
| `themeFrosted` | 毛玻璃 | Frosted |
| `themeSolid` | 纯色 | Solid |
| `themeOrder` | 秩序 | Order |
| `themeNeo` | 新拟态 | Neo |
| `themeFlat` | 扁平 | Flat |
| `themeCustom` | 自定义 | Custom |
| `themePickerLabel` | 选择主题 | Choose Theme |

### Custom Theme (Developer)

| Key | zh-CN | en |
|-----|-------|----|
| `customTheme` | 自定义主题 | Custom Theme |
| `importTheme` | 导入主题 | Import Theme |
| `exportTheme` | 导出当前主题 | Export Current Theme |
| `themeImportSuccess` | 主题导入成功 | Theme imported successfully |
| `themeImportError` | 主题导入失败 | Theme import failed |
| `themeJsonPlaceholder` | 粘贴主题 JSON... | Paste theme JSON... |
| `developerOptions` | 开发者选项 | Developer Options |

## Adding a New Language

### Step 1: Create the Locale File

Create a new file at `src/i18n/locales/{locale-code}.ts`. The file must export an object satisfying the `Translations` type (same keys as `zh-CN.ts`).

Example for Japanese (`ja.ts`):

```typescript
import type { Translations } from "./zh-CN";

const ja: Translations = {
  settingsTitle: "設定",
  settingsCloseAriaLabel: "設定を閉じる",
  settingsGroupGeneral: "一般",
  // ... translate ALL keys from zh-CN.ts
  // TypeScript will error if any key is missing
};

export default ja;
```

### Step 2: Register the Locale

Edit `src/i18n/index.ts`:

```typescript
import zhCN from "./locales/zh-CN";
import en from "./locales/en";
import ja from "./locales/ja";  // Add import

export type Locale = "zh-CN" | "en" | "ja";  // Add to union type

const locales: Record<Locale, Translations> = {
  "zh-CN": zhCN,
  en,
  ja,  // Add to record
};
```

### Step 3: Add Language Option to Settings UI

Add the language to the Settings panel language selector so users can pick it. You will also need to add translation keys for the language name:

In `zh-CN.ts`:
```typescript
languageJapanese: "日本語",
```

In `en.ts`:
```typescript
languageJapanese: "日本語",
```

In `ja.ts`:
```typescript
languageJapanese: "日本語",
```

### Step 4: Verify Type Safety

Run `tsc --noEmit` to confirm that the new locale file implements all required keys. Any missing key will cause a type error.

## Architecture Notes

- The i18n system uses **no external library** -- it is built entirely on Solid.js `createSignal`
- Locale switching is instant (no page reload) because `t()` is a reactive function
- The `Translations` type is derived from `zh-CN.ts` using `Record<TranslationKey, string>`, making Chinese the canonical key source
- All locale files are statically imported (no lazy loading), keeping the bundle simple since there are only 2 locales currently

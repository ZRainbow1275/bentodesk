# BentoDesk 2.0.10

## English

BentoDesk 2.0.10 fixes Zone placement on scaled displays. Creating a Zone from
the tray now converts the cursor from screen pixels into the Main window's
logical coordinates, and every Zone creation path keeps the new Zone inside the
visible workspace.

At startup, previously saved Zone geometry is checked once the real logical
viewport is known. Off-screen or oversized Zones are moved back into view and
the repaired layout is saved atomically; valid in-bounds geometry is unchanged.

## 简体中文

BentoDesk 2.0.10 修复了缩放显示器上的 Zone 定位问题。现在从托盘新建 Zone 时，
会把屏幕像素坐标转换为主窗口逻辑坐标；所有新建入口也会确保 Zone 位于可见工作区内。

应用启动并取得真实逻辑视口后，会检查已保存的 Zone 几何。越界或尺寸异常的 Zone
会被移回可见范围并原子写回，原本位于视口内的合法布局保持不变。

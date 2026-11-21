# RAM VIEW · Rust Edition

> **Real-Time Squarified Treemap RAM Visualizer for Windows** — a 1:1 Rust rewrite of [RAMView](https://github.com/Ahmadhassan806/RAMView).

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/Windows-10%20%2F%2011-0078D6.svg?style=flat-square&logo=windows)](https://www.microsoft.com/windows)

RAM VIEW 把实时的物理内存占用画成一块块半透明玻璃方块：每个进程（或同名进程组）一块，面积按 Working Set 物理内存分配，布局使用 **Squarified Treemap**（Bruls / Huizing / van Wijk）算法，避免出现细长条。

本项目是原 C# / WPF 版本 **1:1 的 Rust 重写**：算法、排序、筛选、分组、防霸占、临界进程保护、托盘、全局热键、本地 HTTP API 与设置文件格式全部对齐；渲染层从 WPF/OpenGL 换成 **Win32 分层窗口 + GDI 软件绘制**，因此常驻内存大幅下降。

---

## 特性

- **实时 Treemap**：每 250–3000 ms 轮询一次，方块按内存权重铺满画布。
- **280 ms 缓动动画**：位置/尺寸插值、进程出现淡入、退出淡出（QuadraticEaseOut），键盘搜索时只淡化不重排。
- **双显示模式**：浮层 HUD（可拖动 / 可缩放）与紧凑任务条（横向滚动）。
- **防霸占与最小面积（§6）**：默认最小视觉面积 20 MB，最大单块占比 60%，可在设置中调节。
- **搜索与筛选**：即时子串搜索、`All / Apps / System / Highest` 预设。
- **进程详情**：点击方块查看 PID、Working Set、占比、实例数、路径，可打开所在目录、复制信息、结束进程。
- **临界进程保护（§23）**：`System / Idle / csrss / wininit / services / smss / lsass / dwm / svchost` 等一律拒绝终止；PID 0/4 直接拦截。
- **玻璃 HUD**：分层窗口整体透明度、置顶、锁定位置、点击穿透（Ghost）。
- **托盘 + 全局热键**：托盘菜单（显示/隐藏、紧凑模式、Ghost、设置、刷新、退出）、实时内存 Tooltip、气泡提示；`Ctrl+Shift+R` 切换显示。
- **本地 HTTP API（51888）**：`/api/snapshot`、`/api/health`、`/api/kill`，带 CORS，供网页可视化前端直接使用。
- **完全离线**：无遥测、无外部请求、无账号。

## 为什么内存这么小

原版（以及第一版 Rust 实现）使用 GPU 渲染栈，进程会映射整套图形驱动（OpenGL/D3D）与字体/纹理资源。这一版：

| 项目 | 做法 |
| --- | --- |
| 窗口 | `WS_EX_LAYERED` + `SetLayeredWindowAttributes`，圆角用窗口 Region 裁剪 |
| 绘制 | GDI 软件绘制到 32bpp DIB，再 `BitBlt` 到屏幕 DC（双缓冲，无闪烁） |
| 字体 | 直接使用系统 Segoe UI / Segoe MDL2 Assets，只缓存实际用到的字号 |
| 图标 | PNG 解码为预乘 BGRA DIB，`AlphaBlend` 贴图（无 GPU 纹理） |
| 搜索框 | 原生 `EDIT` 子控件（中文输入法正常），深色自绘外观 |
| 进程数据 | 不再在快照 / Treemap / 详情里重复克隆进程列表与图标字节 |

`Cargo.toml` 的 release profile 使用 `opt-level = "z"`、LTO、`panic = "abort"`、`strip`，可执行文件约 **0.7 MB**。

> 实际内存占用随系统进程数、图标数量与 DPI 缩放变化，通常在工作集 **15–30 MB** 量级（任务管理器「工作集」）。

---

## 架构

```
RAMView-Rust/
├─ crates/
│  ├─ ramview-core/        # 纯逻辑：模型、SquarifiedTreemap、权重/防霸占、分组筛选、指标、白名单、热键解析
│  ├─ ramview-platform/    # Win32 服务：内存查询、进程枚举、图标提取、进程树终止、设置读写、自启、HTTP API、托盘/热键、剪贴板
│  └─ ramview-app/         # 可执行程序：Win32 消息循环、主窗口/设置窗口、GDI 绘制、动画、交互
├─ Cargo.toml              # workspace
└─ README.md
```

与原项目的对应关系：

| 原项目 | 本仓库 |
| --- | --- |
| `RAMView.Core`（Models / Algorithms / Services / Interfaces） | `crates/ramview-core` |
| `RAMView.Infrastructure`（Native / Services） | `crates/ramview-platform` |
| `RAMView.App`（MainWindow / Controls / Views / Converters） | `crates/ramview-app`（GDI 重绘） |

---

## 快速开始

### 环境

- Windows 10 / 11（x64）
- Rust 1.75+（MSVC toolchain，`x86_64-pc-windows-msvc`）

### 构建

```powershell
git clone https://github.com/s13mjl/RAMView-Rust.git
cd RAMView-Rust

# 开发构建
cargo build

# 发布构建
cargo build --release
```

产物：`target\release\RAMView.exe`（无控制台窗口）。

### 运行

```powershell
.\target\release\RAMView.exe
# 开机自启项会带 --minimized：启动后直接驻留托盘
.\target\release\RAMView.exe --minimized
```

### 测试

一致性测试由原项目的 xUnit 用例逐条移植，覆盖 Treemap、权重/防霸占、临界进程、分组筛选、格式化、热键、指标与设置 JSON 兼容性：

```powershell
cargo test
```

---

## 使用

- **拖动**：在空白/标题区域按住左键拖动；勾选 `🔒` 后锁定。
- **缩放**：拖动窗口边缘（最小 340×180，紧凑模式最小高度 60）。
- **搜索**：右上角输入框，非匹配方块淡化到 22%，布局不跳动。
- **筛选**：底部 `All / Apps / System / Highest`。
- **分组**：底部 `Group` 合并同名进程（如多个 `chrome`），块上显示 `×N`。
- **Pin**：置顶 HUD；**Ghost**：鼠标点击穿透整个窗口（用热键或托盘恢复）。
- **详情**：点击方块 → 查看信息 / 打开目录 / 复制 / 结束进程（二次确认）。
- **托盘**：左键双击显示/隐藏，右键打开菜单；Tooltip 实时显示 `已用 / 总量 (百分比)`。
- **热键**：`Ctrl+Shift+R` 显示/隐藏（可在设置文件 `GlobalHotkey` 中修改后重启生效）。

---

## 本地 HTTP API

默认监听 `127.0.0.1:51888` 与 `[::1]:51888`，所有响应带 CORS 头。

### `GET /api/snapshot`

```json
{
  "totalPhysicalBytes": 34273861632,
  "usedPhysicalBytes": 21881787392,
  "availablePhysicalBytes": 12392074240,
  "systemMemoryUsagePercentage": 63.85,
  "summaryText": "20.38 GB / 31.92 GB",
  "formattedTotal": "31.92 GB",
  "formattedUsed": "20.38 GB",
  "formattedAvailable": "11.54 GB",
  "processes": [
    {
      "processId": 1234,
      "processName": "chrome",
      "workingSet64": 912345678,
      "percentageOfTracked": 12.4,
      "executablePath": "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
      "iconPngBytes": "<base64>",
      "isResponding": true,
      "startTime": "2026-11-21T09:00:00+08:00",
      "timestamp": "2026-11-21T10:30:00.123456Z",
      "instanceCount": 12,
      "formattedMemory": "870.1 MB",
      "formattedPercentage": "12.4%"
    }
  ]
}
```

### `POST /api/kill`

```powershell
curl.exe -X POST http://localhost:51888/api/kill `
  -H "Content-Type: application/json" `
  -d "{\"pid\":1234}"
```

成功：`{"success":true,"message":"Process 'chrome' (PID 1234) terminated"}`；受保护进程返回 403。

### `GET /api/health`

```json
{"status":"ok","service":"RAMView Local Agent","port":51888}
```

原项目的 `web/` 网页可视化前端可以直接连接本服务（接口字段与 CORS 行为保持一致）。

---

## 设置

设置保存在 `%LOCALAPPDATA%\RAMView\settings.json`，字段名与原版 System.Text.Json 输出完全一致（PascalCase），已保存的文件可直接沿用。

```json
{
  "RefreshIntervalMs": 1000,
  "IsGrouped": true,
  "AlwaysOnTop": true,
  "ClickThrough": false,
  "LockPosition": false,
  "StartWithWindows": false,
  "WindowOpacity": 0.96,
  "MinMemoryFloorBytes": 20971520,
  "AntiDominanceMaxAreaShare": 0.60,
  "ShowSystemProcesses": true,
  "AppMode": "Overlay",
  "OverlayLeft": 100,
  "OverlayTop": 100,
  "OverlayWidth": 540,
  "OverlayHeight": 380,
  "HighMemoryThresholdPercent": 80.0,
  "CriticalMemoryThresholdPercent": 90.0,
  "Theme": "Dark",
  "FilterPreset": "All",
  "GlobalHotkey": "Ctrl+Shift+R"
}
```

设置窗口可调整：刷新间隔、整体不透明度、防霸占上限、置顶、点击穿透、锁定位置、进程分组、显示系统进程、开机自启。保存后立即应用（刷新间隔与原版一致，重启后生效）。

---

## 与原版的差异

1. **紧凑模式窗口高度 120 DIP**（原版 80 会把底部控件挤出窗口、条带几乎没有可见高度），并隐藏底部栏与 Ghost 横幅。
2. **启动时不从磁盘加载设置**：与原版行为保持一致（原版 `LoadSettingsAsync` 从未被调用），保存功能正常。如需启动即加载，改一行即可。
3. **视觉细节**：圆角由窗口 Region 裁剪（无抗锯齿边缘）、没有窗口投影、Tooltip 在 HUD 内部绘制；图标使用系统 Segoe MDL2 Assets 单色字形替代彩色 Emoji；告警/确认使用原生 `MessageBoxW`。
4. **搜索框**改为原生 `EDIT` 控件，中文输入法、选择、复制粘贴均为系统行为。

除此之外的功能、算法与 API 行为均与原版对齐。

---

## 安全说明

- 进程终止前会重新校验名称与 PID，命中白名单直接拒绝。
- 结束进程使用 `Kill(entireProcessTree: true)` 等价语义：先终止子进程再终止目标进程，并最多等待 2 秒。
- 本地 API 仅绑定回环地址，并带有独立的受保护进程名单；终止失败返回 403/500 与原因。

---

## 许可与致谢

本项目采用 MIT 许可证，见 [LICENSE](LICENSE)。

- 原始设计与 C# / WPF 实现：[Ahmadhassan806/RAMView](https://github.com/Ahmadhassan806/RAMView)（MIT）
- Rust 重写：s13mjl

---

## English Summary

RAM VIEW is a Windows desktop utility that visualizes live RAM usage as a squarified treemap of translucent glass tiles. This repository is a 1:1 Rust rewrite of the original WPF app, keeping the layout algorithm, filters, grouping, anti-dominance rule, protected-process denylist, tray/hotkey behavior, settings format and the local HTTP API on port 51888. The HUD is rendered by Win32 GDI (software) instead of a GPU stack, which keeps the executable around 0.7 MB and the working set in the 15–30 MB range. Build with `cargo build --release`.

//! 全屏提醒覆盖窗口：按提醒样式定尺寸的不透明窗口（虚拟机/远程桌面友好）
//! - 启动时预建隐藏，页面监听器提前就绪
//! - 显示顺序：先 emit（隐藏状态下渲染）→ 延时后显示 → 定时隐藏
use crate::config::AppConfig;
use crate::protocol::IncomingMessage;
use serde::Serialize;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

#[derive(Serialize, Clone)]
pub struct ShowPayload {
    pub style: String,
    pub from: String,
    pub text: String,
    pub duration: u64,
}

/// 各样式对应的窗口尺寸/位置（物理像素）
fn geometry(style: &str, mw: i32, mh: i32) -> (i32, i32, i32, i32) {
    match style {
        // 顶部胶囊：顶部居中横条
        "capsule" => {
            let w = (mw * 3 / 5).min(1150);
            (w, 110, (mw - w) / 2, 8)
        }
        // CI 接管：顶部通栏
        "class_island" => (mw - 40, 150, 20, 0),
        // CW 接管：居中卡片
        "class_widget" => ((mw * 2 / 5).min(620), 340, (mw - (mw * 2 / 5).min(620)) / 2, (mh - 340) / 3),
        // 全屏划入：整屏
        _ => (mw, mh, 0, 0),
    }
}

/// 应用启动时预建隐藏的覆盖窗口（页面监听器随页面加载就绪）
pub fn ensure(app: &tauri::AppHandle) {
    if app.get_webview_window("overlay").is_some() {
        return;
    }
    let builder = tauri::WebviewWindowBuilder::new(
        app,
        "overlay",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Class Broadcast 提醒")
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false) // 预建但隐藏
    .inner_size(320.0, 200.0)
    .position(0.0, 0.0)
    .initialization_script("window.__CB_OVERLAY__ = true;");
    if let Err(e) = builder.build() {
        eprintln!("[ClassBroadcast] 创建覆盖窗口失败: {e}");
        return;
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_ignore_cursor_events(true); // 鼠标穿透
    }
}

/// 展示提醒：隐藏状态下先渲染 → 显示 → 定时隐藏
pub async fn show(app: &tauri::AppHandle, config: &Arc<RwLock<AppConfig>>, msg: &IncomingMessage) {
    let (style, duration) = {
        let c = config.read().await;
        (c.reminder_style.as_str().to_string(), 8000u64)
    };
    let Some(win) = app.get_webview_window("overlay") else {
        return;
    };

    // 按样式调整窗口几何（隐藏状态下完成）
    let mon = win.current_monitor().ok().flatten();
    if let Some(mon) = &mon {
        let (w, h, x, y) = geometry(&style, mon.size().width as i32, mon.size().height as i32);
        let _ = win.set_size(tauri::PhysicalSize::new(w.max(100), h.max(80)));
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }

    // 先渲染（窗口隐藏时前端即可收到事件），再显示
    let payload = ShowPayload {
        style: style.clone(),
        from: msg.from.clone(),
        text: msg.text.clone(),
        duration,
    };
    let _ = app.emit_to("overlay", "bc://show", &payload);
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let _ = win.show();

    // 后端兜底隐藏
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(duration + 900)).await;
        if let Some(w) = app2.get_webview_window("overlay") {
            let _ = w.hide();
        }
    });
}

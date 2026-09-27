use base64::Engine as _;
use crate::cloud;
use crate::config::{AppConfig, ConfigStore, Mode};
use crate::lan::{self, LanState, SendResult};
use crate::protocol::Packet;
use std::sync::{Arc, Mutex, RwLock as StdRwLock};
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

pub struct ManagedState {
    pub config: Arc<tokio::sync::RwLock<AppConfig>>,
    pub store: ConfigStore,
    pub lan: Arc<LanState>,
    /// 最近收到的消息（供前端刷新时拉取历史）
    pub inbox: Arc<Mutex<Vec<crate::protocol::IncomingMessage>>>,
    pub last_cloud_poll: Arc<StdRwLock<i64>>,
}

#[tauri::command]
async fn get_config(state: State<'_, ManagedState>) -> Result<AppConfig, String> {
    Ok(state.config.read().await.clone())
}

#[derive(serde::Serialize)]
pub struct SendReport {
    pub results: Vec<SendResult>,
    pub cloud_ok: Option<bool>,
    pub cloud_detail: String,
}

/// 教师端发送广播：立即返回「已加入投递队列」，后台自动重试并按班级回报结果（bc://send-result）
#[tauri::command]
async fn send_broadcast(
    app: AppHandle,
    state: State<'_, ManagedState>,
    class_ids: Vec<String>,
    sender_name: String,
    text: String,
) -> Result<SendReport, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("消息内容不能为空".into());
    }
    if text.len() > crate::protocol::MAX_TEXT_LEN {
        return Err(format!("消息过长（上限 {} 字符）", crate::protocol::MAX_TEXT_LEN));
    }
    let sender = if sender_name.trim().is_empty() {
        state.config.read().await.sender_name.clone()
    } else {
        sender_name.trim().to_string()
    };

    let targets = lan::targets_for(&state.lan, &class_ids);
    let mut queued: Vec<SendResult> = Vec::new();
    for (peer, classes) in &targets {
        for c in classes {
            queued.push(SendResult {
                class_id: c.id.clone(),
                class_name: c.name.clone(),
                ok: true,
                detail: "投递中…".into(),
            });
        }
    }

    // 后台投递 + 重试，逐班级回报最终结果
    let app2 = app.clone();
    let state2 = state.lan.clone();
    let sender2 = sender.clone();
    tauri::async_runtime::spawn(async move {
        let cfg = state2.config.read().await.clone();
        let mut pkt = Packet::new("msg", &cfg.device_id);
        pkt.from = sender2.clone();
        pkt.text = text.clone();
        pkt.sign(&cfg.shared_key);

        for (peer, classes) in lan::targets_for(&state2, &class_ids) {
            let res = lan::deliver_retry(&peer.ip, peer.tcp_port, &pkt, 3).await;
            for c in classes {
                let sr = SendResult {
                    class_id: c.id.clone(),
                    class_name: c.name.clone(),
                    ok: res.is_ok(),
                    detail: match &res {
                        Ok(_) => "已送达".into(),
                        Err(e) if e == "receipt-timeout" => "已发送（未收到回执）".into(),
                        Err(e) => format!("失败: {e}"),
                    },
                };
                let _ = app2.emit("bc://send-result", &sr);
            }
        }

        // 云端链路（可选，不阻塞局域网结果）
        if cfg.cloud_enabled && !cfg.cloud_relay_url.trim().is_empty() {
            let _ = cloud::send_cloud(&state2, &sender2, &text).await;
        }
    });

    Ok(SendReport {
        results: queued,
        cloud_ok: None,
        cloud_detail: String::new(),
    })
}

/// 教师端拉取当前在线大屏节点
#[tauri::command]
async fn list_peers(state: State<'_, ManagedState>) -> Result<Vec<lan::PeerInfo>, String> {
    let map = state.lan.peers.lock().unwrap();
    Ok(map.values().cloned().collect())
}

/// 拉取收件箱（大屏端历史消息）
#[tauri::command]
async fn get_inbox(
    state: State<'_, ManagedState>,
) -> Result<Vec<crate::protocol::IncomingMessage>, String> {
    let inbox = state.inbox.lock().unwrap();
    Ok(inbox.iter().rev().take(200).cloned().collect())
}

/// 本机局域网 IP（前端展示用）
#[tauri::command]
fn get_local_ip() -> Result<String, String> {
    Ok(lan::local_ipv4().unwrap_or_else(|| "未知".into()))
}

/// 修改受保护配置 / 退出前校验：SHA-256(password) == 存储哈希
fn verify_pw(hash: &str, password: &str) -> bool {
    if hash.is_empty() {
        return true;
    }
    use sha2::Digest;
    let h = hex::encode(sha2::Sha256::digest(password.as_bytes()));
    h == hash
}

/// 保存配置（若已设置保护密码，需携带正确密码；固定角色构建强制角色）
#[tauri::command]
async fn save_config(
    state: State<'_, ManagedState>,
    config: AppConfig,
    auth_password: Option<String>,
) -> Result<(), String> {
    let old_hash = state.config.read().await.settings_password_hash.clone();
    if !old_hash.is_empty() {
        let auth = auth_password.unwrap_or_default();
        if !verify_pw(&old_hash, &auth) {
            return Err("需要设置保护密码验证".into());
        }
    }
    let mut cfg = config;
    cfg.normalize();
    // 固定角色：不允许通过保存配置改角色
    if cfg!(feature = "teacher-ui") {
        cfg.mode = crate::config::Mode::Teacher;
    }
    state.store.save(&cfg)?;
    *state.config.write().await = cfg;
    Ok(())
}

/// 托盘/前端退出（受保护时需密码）
#[tauri::command]
async fn exit_app(
    app: AppHandle,
    state: State<'_, ManagedState>,
    password: Option<String>,
) -> Result<(), String> {
    let hash = state.config.read().await.settings_password_hash.clone();
    if !hash.is_empty() && !verify_pw(&hash, &password.unwrap_or_default()) {
        let _ = app.emit("bc://exit-denied", ());
        return Err("密码错误".into());
    }
    app.exit(0);
    Ok(())
}

/// 是否已设置保护密码（前端决定是否弹验证框）
#[tauri::command]
async fn has_password(state: State<'_, ManagedState>) -> Result<bool, String> {
    Ok(!state.config.read().await.settings_password_hash.is_empty())
}

/// ── 远程监控 ──
/// 构造监控请求行
async fn monitor_line(state: &ManagedState, action: String) -> Result<String, String> {
    let cfg = state.config.read().await.clone();
    let mut pkt = Packet::new("monitor", &cfg.device_id);
    pkt.text = action;
    pkt.sign(&cfg.shared_key);
    let mut line = serde_json::to_string(&pkt).map_err(|e| e.to_string())?;
    line.push('\n');
    Ok(line)
}

/// 实时帧（最高清晰度），返回 base64 JPEG
#[tauri::command]
async fn monitor_frame(
    state: State<'_, ManagedState>,
    ip: String,
    port: Option<u16>,
) -> Result<String, String> {
    let cfg = state.config.read().await.clone();
    let line = monitor_line(&state, "frame".into()).await?;
    let port = port.filter(|p| *p > 0).unwrap_or(cfg.lan_port + 1);
    monitor_request(&ip, port, line).await
}

/// 远程鼠标点击
#[tauri::command]
async fn monitor_click(
    state: State<'_, ManagedState>,
    ip: String,
    port: Option<u16>,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let cfg = state.config.read().await.clone();
    let line = monitor_line(&state, format!("click:{x},{y}")).await?;
    let port = port.filter(|p| *p > 0).unwrap_or(cfg.lan_port + 1);
    monitor_request(&ip, port, line).await.map(|_| ())
}

/// 远程键盘输入
#[tauri::command]
async fn monitor_type(
    state: State<'_, ManagedState>,
    ip: String,
    port: Option<u16>,
    text: String,
) -> Result<(), String> {
    let cfg = state.config.read().await.clone();
    let line = monitor_line(&state, format!("type:{text}")).await?;
    let port = port.filter(|p| *p > 0).unwrap_or(cfg.lan_port + 1);
    monitor_request(&ip, port, line).await.map(|_| ())
}

/// 全分辨率截图：请求大屏抓屏并保存到本机图片目录，返回文件路径
#[tauri::command]
async fn monitor_screenshot(
    app: AppHandle,
    state: State<'_, ManagedState>,
    ip: String,
    port: Option<u16>,
) -> Result<String, String> {
    let cfg = state.config.read().await.clone();
    let mut pkt = Packet::new("monitor", &cfg.device_id);
    pkt.text = "shot".into();
    pkt.sign(&cfg.shared_key);
    let mut line = serde_json::to_string(&pkt).map_err(|e| e.to_string())?;
    line.push('\n');
    let port = port.filter(|p| *p > 0).unwrap_or(cfg.lan_port + 1);
    let b64 = monitor_request(&ip, port, line).await?;
    let jpg = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| e.to_string())?;
    let dir = app.path().picture_dir().map_err(|e| e.to_string())?;
    let path = dir.join(format!(
        "CB-截图-{}.jpg",
        chrono_free_name()
    ));
    std::fs::write(&path, jpg).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

fn chrono_free_name() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    format!("{t}")
}

/// 监控 TCP 请求：连接大屏、发送一行、读一行回包，返回帧 base64
async fn monitor_request(ip: &str, port: u16, line: String) -> Result<String, String> {
    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        tokio::net::TcpStream::connect((ip, port)),
    )
    .await
    .map_err(|_| "连接超时".to_string())?
    .map_err(|e| e.to_string())?;
    let (reader, mut writer) = stream.into_split();
    writer.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())?;
    let mut lines = tokio::io::BufReader::new(reader).lines();
    let ack = tokio::time::timeout(std::time::Duration::from_secs(6), lines.next_line())
        .await
        .map_err(|_| "响应超时".to_string())?
        .map_err(|e| e.to_string())?;
    let ack: Packet = serde_json::from_str(&ack.ok_or("连接被关闭")?).map_err(|e| e.to_string())?;
    if ack.nonce == "ok" {
        Ok(ack.text)
    } else {
        Err(ack.text)
    }
}

/// 固定角色构建标记（教师端 exe = "teacher"，大屏端 = null）
#[tauri::command]
fn get_build_role() -> Option<&'static str> {
    if cfg!(feature = "teacher-ui") {
        Some("teacher")
    } else {
        None
    }
}

/// 手动添加大屏 IP（跨网段 / 虚拟机 NAT 广播不可达时使用）：发 probe 获取 announce 并注册
#[tauri::command]
async fn add_peer_by_ip(state: State<'_, ManagedState>, ip: String, port: Option<u16>) -> Result<String, String> {
    let cfg = state.config.read().await.clone();
    let port = port.unwrap_or(cfg.lan_port);
    let mut pkt = Packet::new("probe", &cfg.device_id);
    pkt.sign(&cfg.shared_key);
    let bytes = serde_json::to_vec(&pkt).map_err(|e| e.to_string())?;
    let sock = tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await.map_err(|e| e.to_string())?;
    sock.send_to(&bytes, (ip.as_str(), port)).await.map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let (len, addr) = tokio::time::timeout(std::time::Duration::from_secs(3), sock.recv_from(&mut buf))
        .await
        .map_err(|_| "无响应（检查 IP / 端口 / 防火墙）".to_string())?
        .map_err(|e| e.to_string())?;
    let p: Packet = serde_json::from_slice(&buf[..len]).map_err(|e| e.to_string())?;
    if p.kind != "announce" || !p.verify(&cfg.shared_key) {
        return Err("响应校验失败（频道密钥不一致？）".into());
    }
    let name = p.class_name.clone();
    {
        let mut map = state.lan.peers.lock().unwrap();
        map.insert(p.device_id.clone(), lan::PeerInfo {
            device_id: p.device_id.clone(),
            class_id: p.class_id.clone(),
            class_name: p.class_name.clone(),
            classes: p.classes.clone(),
            groups: p.groups.clone(),
            ip: addr.ip().to_string(),
            last_seen: crate::protocol::now_ms(),
            tcp_port: if p.tcp_port > 0 { p.tcp_port } else { cfg.lan_port + 1 },
        });
    }
    Ok(name)
}

pub fn run(build_role: Option<crate::config::Mode>) {
    tauri::Builder::default()
        .setup(move |app| {
            // 按角色隔离配置目录（双端同机测试互不干扰）
            let role_dir = match build_role {
                Some(crate::config::Mode::Teacher) => "teacher",
                _ => "screen",
            };
            let dir = app
                .path()
                .app_data_dir()
                .expect("无法定位应用数据目录")
                .join(role_dir);
            let store = ConfigStore::new(dir);
            let mut cfg = store.load();
            cfg.normalize();
            // 固定角色构建：强制角色
            if let Some(m) = build_role {
                cfg.mode = m;
            }

            let config = Arc::new(tokio::sync::RwLock::new(cfg.clone()));
            let lan_state = Arc::new(LanState::new(config.clone()));

            let managed = ManagedState {
                config: config.clone(),
                store,
                lan: lan_state.clone(),
                inbox: Arc::new(Mutex::new(Vec::new())),
                last_cloud_poll: Arc::new(StdRwLock::new(0)),
            };
            app.manage(managed);

            // 后端运行时
            let app_handle = app.handle().clone();
            let rt_lan = lan_state.clone();
            tauri::async_runtime::spawn(async move {
                lan::start(app_handle.clone(), (*rt_lan).clone());
            });
            let app_handle2 = app.handle().clone();
            let rt_lan2 = lan_state.clone();
            tauri::async_runtime::spawn(async move {
                cloud::start(app_handle2, rt_lan2);
            });

            // 将收到的消息写入收件箱
            let inbox_handle = app.handle().clone();
            app.listen("bc://incoming", move |event| {
                if let Ok(msg) =
                    serde_json::from_str::<crate::protocol::IncomingMessage>(event.payload())
                {
                    let managed: State<ManagedState> = inbox_handle.state();
                    let mut inbox = managed.inbox.lock().unwrap();
                    inbox.push(msg);
                    if inbox.len() > 500 {
                        inbox.remove(0);
                    }
                }
            });

            // 预建隐藏的全屏提醒覆盖层
            crate::overlay::ensure(&app.handle().clone());

            // 系统托盘：左键/菜单显示主窗口，退出走密码校验流程
            use tauri::menu::{Menu, MenuItem};
            let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            tauri::tray::TrayIconBuilder::with_id("cb-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Class Broadcast")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        let _ = app.emit("bc://quit-request", ());
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = ev {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        // 关闭窗口 = 隐藏到托盘
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            send_broadcast,
            list_peers,
            get_inbox,
            get_local_ip,
            exit_app,
            has_password,
            monitor_frame,
            monitor_screenshot,
            monitor_click,
            monitor_type,
            get_build_role,
            add_peer_by_ip
        ])
        .run(tauri::generate_context!())
        .expect("Class Broadcast 启动失败");
}

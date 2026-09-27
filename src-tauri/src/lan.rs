use crate::config::{AppConfig, Mode};
use crate::protocol::{now_ms, ClassDto, GroupDto, IncomingMessage, Packet, PEER_TIMEOUT_MS};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::RwLock;

/// 已发现的大屏（班级）节点
#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerInfo {
    pub device_id: String,
    pub class_id: String,
    pub class_name: String,
    pub classes: Vec<ClassDto>,
    pub groups: Vec<GroupDto>,
    pub ip: String,
    pub last_seen: i64,
    /// 大屏实际 TCP 监听端口
    pub tcp_port: u16,
}

/// 全局共享：本机 TCP 监听端口（announce 用）
pub static MY_TCP_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

/// 单个发送结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct SendResult {
    pub class_id: String,
    pub class_name: String,
    pub ok: bool,
    pub detail: String,
}

/// 状态全部为 Arc，Clone 后仍共享底层数据
#[derive(Clone)]
pub struct LanState {
    pub config: Arc<RwLock<AppConfig>>,
    /// 教师端发现的节点缓存：device_id -> PeerInfo
    pub peers: Arc<Mutex<HashMap<String, PeerInfo>>>,
    /// 已处理消息 id（去重：重试不会重复弹窗）
    pub msg_ids: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl LanState {
    pub fn new(config: Arc<RwLock<AppConfig>>) -> Self {
        Self {
            config,
            peers: Arc::new(Mutex::new(HashMap::new())),
            msg_ids: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }
}

/// 本机局域网 IPv4（通过 UDP connect 探测路由出口）
pub fn local_ipv4() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    s.local_addr().ok().map(|a| a.ip().to_string())
}

/// 启动局域网运行时（UDP 发现 + TCP 消息通道 + 大屏 announce）
pub fn start(app: AppHandle, state: LanState) {
    // UDP：大屏 announce 广播 + 教师端接收发现
    tokio::spawn(udp_task(app.clone(), state.config.clone(), state.peers.clone()));
    // TCP：所有模式都监听消息通道（教师端也可被投递，便于回执/测试）
    tokio::spawn(tcp_server(app.clone(), state.config.clone(), state.msg_ids.clone()));
    // 定期清理过期节点
    tokio::spawn(cleanup_task(state.peers.clone()));
}

async fn udp_task(
    app: AppHandle,
    config: Arc<RwLock<AppConfig>>,
    peers: Arc<Mutex<HashMap<String, PeerInfo>>>,
) {
    let port = config.read().await.lan_port;
    let socket: std::sync::Arc<UdpSocket> =
        std::sync::Arc::new(bind_udp(port).expect("无法绑定 UDP 端口（可能被占用）"));
    let mut buf = vec![0u8; 64 * 1024];

    // 大屏端周期 announce
    let announce_cfg = config.clone();
    let announce_sock = socket.clone();
    tokio::spawn(async move {
        loop {
            let (cfg, key) = {
                let c = announce_cfg.read().await;
                (c.clone(), c.shared_key.clone())
            };
            if cfg.mode == Mode::Screen {
                let mut pkt = Packet::new("announce", &cfg.device_id);
                pkt.class_id = cfg.class_id.clone();
                pkt.class_name = cfg.class_name.clone();
                pkt.classes = cfg
                    .classes
                    .iter()
                    .map(|c| ClassDto {
                        id: c.id.clone(),
                        name: c.name.clone(),
                    })
                    .collect();
                pkt.groups = cfg
                    .groups
                    .iter()
                    .map(|g| GroupDto {
                        id: g.id.clone(),
                        name: g.name.clone(),
                        class_ids: g.class_ids.clone(),
                    })
                    .collect();
                pkt.sign(&key);
                pkt.tcp_port = std::sync::atomic::AtomicU16::load(
                    &MY_TCP_PORT,
                    std::sync::atomic::Ordering::Relaxed,
                );
                if let Ok(bytes) = serde_json::to_vec(&pkt) {
                    let _ = announce_sock
                        .send_to(&bytes, ("255.255.255.255", port))
                        .await;
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    loop {
        let (len, addr) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let pkt: Packet = match serde_json::from_slice(&buf[..len]) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let (cfg, key) = {
            let c = config.read().await;
            (c.clone(), c.shared_key.clone())
        };
        // 大屏端收到 probe：单播回 announce（供教师端手动添加 IP）
        if pkt.kind == "probe" && pkt.verify(&key) && cfg.mode == Mode::Screen {
            let mut reply = Packet::new("announce", &cfg.device_id);
            reply.class_id = cfg.class_id.clone();
            reply.class_name = cfg.class_name.clone();
            reply.classes = cfg
                .classes
                .iter()
                .map(|c| ClassDto { id: c.id.clone(), name: c.name.clone() })
                .collect();
            reply.groups = cfg
                .groups
                .iter()
                .map(|g| GroupDto {
                    id: g.id.clone(),
                    name: g.name.clone(),
                    class_ids: g.class_ids.clone(),
                })
                .collect();
            reply.sign(&key);
            if let Ok(bytes) = serde_json::to_vec(&reply) {
                let _ = socket.send_to(&bytes, addr).await;
            }
            continue;
        }
        if pkt.kind != "announce" || !pkt.verify(&key) {
            continue;
        }
        let own_device = cfg.device_id.clone();
        if pkt.device_id == own_device {
            continue; // 忽略自己（按 device_id，同机双实例测试也能互相发现）
        }
        let ip = addr.ip().to_string();
        let peer = PeerInfo {
            device_id: pkt.device_id.clone(),
            class_id: pkt.class_id.clone(),
            class_name: pkt.class_name.clone(),
            classes: pkt.classes.clone(),
            groups: pkt.groups.clone(),
            ip,
            last_seen: now_ms(),
            tcp_port: if pkt.tcp_port > 0 { pkt.tcp_port } else { config.read().await.lan_port + 1 },
        };
        {
            let mut map = peers.lock().unwrap();
            map.insert(peer.device_id.clone(), peer);
        }
        // 实时刷新：立即通知前端节点列表变化
        let _ = tauri::Emitter::emit(&app, "bc://peers-changed", ());
    }
}

async fn tcp_server(app: AppHandle, config: Arc<RwLock<AppConfig>>, msg_ids: Arc<Mutex<std::collections::HashSet<String>>>) {
    let base = config.read().await.lan_port + 1;
    // 独占绑定：同机多实例自动顺延端口（并把实际端口写进 announce），杜绝端口劫持
    let mut listener = None;
    let mut used = base;
    for p in base..base + 10 {
        match tcp_listener(p) {
            Ok(l) => {
                listener = Some(l);
                used = p;
                break;
            }
            Err(_) => continue,
        }
    }
    let Some(std_listener) = listener else {
        eprintln!("[ClassBroadcast] TCP 监听失败（{base}~{} 均被占用）", base + 9);
        return;
    };
    std::sync::atomic::AtomicU16::store(
        &MY_TCP_PORT,
        used,
        std::sync::atomic::Ordering::Relaxed,
    );
    let listener = match TcpListener::from_std(std_listener) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[ClassBroadcast] TCP 初始化失败: {e}");
            return;
        }
    };
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let app = app.clone();
        let config = config.clone();
        let msg_ids = msg_ids.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_tcp_conn(app, config, msg_ids, stream).await {
                eprintln!("[ClassBroadcast] 连接处理失败: {e}");
            }
        });
    }
}

async fn handle_tcp_conn(
    app: AppHandle,
    config: Arc<RwLock<AppConfig>>,
    msg_ids: Arc<Mutex<std::collections::HashSet<String>>>,
    stream: TcpStream,
) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if line.len() > 100 * 1024 {
            break; // 防超大包
        }
        let pkt: Packet = match serde_json::from_str(&line) {
            Ok(p) => p,
            Err(_) => continue,
        };
        // 远程监控通道：frame（高清帧）/ shot（截图）/ click:x,y / type:文本，需签名校验
        if pkt.kind == "monitor" {
            let key = config.read().await.shared_key.clone();
            if !pkt.verify(&key) {
                continue;
            }
            let action = pkt.text.clone();
            let parse_result = || -> Result<String, String> {
                if let Some(rest) = action.strip_prefix("click:") {
                    let (xs, ys) = rest.split_once(',').ok_or("参数错误")?;
                    crate::capture::send_click(xs.trim().parse::<i32>().map_err(|_| "参数错误")?, ys.trim().parse::<i32>().map_err(|_| "参数错误")?).map(|_| String::new())
                } else if let Some(s) = action.strip_prefix("type:") {
                    crate::capture::send_text(s).map(|_| String::new())
                } else {
                    // frame / shot：全分辨率高清帧
                    crate::capture::grab_jpeg_base64(1, 85)
                }
            };
            let mut ack = Packet::new("frame", &pkt.device_id);
            match parse_result() {
                Ok(data) => {
                    ack.text = data;
                    ack.nonce = "ok".into();
                }
                Err(e) => {
                    ack.nonce = "err".into();
                    ack.text = e;
                }
            }
            let mut line_out = serde_json::to_string(&ack).unwrap_or_default();
            line_out.push('\n');
            let _ = writer.write_all(line_out.as_bytes()).await;
            let _ = writer.flush().await;
            continue;
        }
        let key = config.read().await.shared_key.clone();
        if pkt.kind != "msg" || !pkt.verify(&key) {
            let _ = writer.write_all(b"{\"kind\":\"ack\",\"ok\":false}\n").await;
            continue;
        }
        let is_new = {
            let mut ids = msg_ids.lock().unwrap();
            ids.insert(pkt.id.clone())
        };
        let _ = writer.write_all(b"{\"kind\":\"ack\",\"ok\":true}\n").await;
        let _ = writer.flush().await;
        if !is_new {
            continue;
        }
        {
            let mut ids = msg_ids.lock().unwrap();
            if ids.len() > 500 {
                ids.clear();
            }
        }
        let incoming = IncomingMessage {
            id: pkt.id.clone(),
            from: pkt.from.clone(),
            text: pkt.text.clone(),
            ts: pkt.ts,
            via: "lan".into(),
        };
        // 回执已即时返回，再触发界面/覆盖层（不阻塞回执）
        let _ = app.emit("bc://incoming", &incoming);
        let app2 = app.clone();
        let config2 = config.clone();
        let incoming2 = incoming.clone();
        tauri::async_runtime::spawn(async move {
            crate::overlay::show(&app2, &config2, &incoming2).await;
        });
    }
    Ok(())
}

/// 收集目标：device -> 命中的班级列表
pub fn targets_for(state: &LanState, class_ids: &[String]) -> Vec<(PeerInfo, Vec<ClassDto>)> {
    let map = state.peers.lock().unwrap();
    let mut out = Vec::new();
    for peer in map.values() {
        let hit: Vec<ClassDto> = peer
            .classes
            .iter()
            .filter(|c| class_ids.contains(&c.id))
            .cloned()
            .collect();
        if !hit.is_empty() {
            out.push((peer.clone(), hit));
        }
    }
    out
}

/// 教师端发送广播：向目标班级所在大屏逐个建立 TCP 连接投递（带自动重试，送达即停）
pub async fn send_lan(
    state: &LanState,
    class_ids: &[String],
    from: &str,
    text: &str,
) -> Vec<SendResult> {
    let cfg = state.config.read().await.clone();
    let targets = targets_for(state, class_ids);

    let mut results = Vec::new();
    let mut pkt = Packet::new("msg", &cfg.device_id);
    pkt.from = from.into();
    pkt.text = text.into();
    pkt.sign(&cfg.shared_key);

    for (peer, classes) in targets {
        let res = deliver_retry(&peer.ip, peer.tcp_port, &pkt, 3).await;
        for c in classes {
            results.push(SendResult {
                class_id: c.id,
                class_name: c.name,
                ok: res.is_ok(),
                detail: match &res {
                    Ok(_) => "已送达".into(),
                    Err(e) if e == "receipt-timeout" => "已发送（未收到回执）".into(),
                    Err(e) => format!("失败: {e}"),
                },
            });
        }
    }
    results
}

async fn deliver_tcp(ip: &str, port: u16, pkt: &Packet) -> Result<(), String> {
    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect((ip, port)),
    )
    .await
    .map_err(|_| "连接超时".to_string())?
    .map_err(|e| e.to_string())?;

    let (reader, mut writer) = stream.into_split();
    let mut line = serde_json::to_string(pkt).map_err(|e| e.to_string())?;
    line.push('\n');
    // 写入也限时，杜绝永久阻塞
    tokio::time::timeout(Duration::from_secs(5), async {
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| "写入超时".to_string())?
    .map_err(|e| e.to_string())?;

    // 等待回执（宽松：超时不算失败，消息已投出）
    let mut lines = BufReader::new(reader).lines();
    let ack = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
        .await
        .map_err(|_| "receipt-timeout".to_string())?
        .map_err(|e| e.to_string())?;
    match ack {
        Some(s) if s.contains("\"ok\":true") => Ok(()),
        Some(s) if s.contains("\"ok\":false") => Err("对端拒收（签名校验失败）".into()),
        Some(_) => Ok(()),
        None => Err("receipt-timeout".into()),
    }
}

/// 带自动重试的投递：送达即停；全部失败返回最后一次错误
pub async fn deliver_retry(ip: &str, port: u16, pkt: &Packet, attempts: u32) -> Result<(), String> {
    let mut last = String::from("未知错误");
    for i in 0..attempts {
        match deliver_tcp(ip, port, pkt).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e;
                // 对端明确拒收（密钥不一致），重试无意义
                if last.contains("拒收") {
                    return Err(last);
                }
                if i + 1 < attempts {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                }
            }
        }
    }
    Err(last)
}

async fn cleanup_task(peers: Arc<Mutex<HashMap<String, PeerInfo>>>) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let now = now_ms();
        peers
            .lock()
            .unwrap()
            .retain(|_, p| now - p.last_seen < PEER_TIMEOUT_MS);
    }
}

/// 绑定 TCP 监听（独占端口，不使用 SO_REUSEADDR，防止同机劫持）
fn tcp_listener(port: u16) -> std::io::Result<std::net::TcpListener> {
    let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
    let sock = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    sock.bind(&addr.into())?;
    sock.listen(64)?;
    Ok(sock.into())
}

/// 绑定 UDP（SO_REUSEADDR，允许同机多实例共存）
fn bind_udp(port: u16) -> std::io::Result<UdpSocket> {
    let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
    let sock = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    sock.set_reuse_address(true)?;
    #[cfg(unix)]
    sock.set_reuse_port(true)?;
    sock.bind(&addr.into())?;
    sock.set_broadcast(true)?;
    sock.set_nonblocking(true)?;
    let s: std::net::UdpSocket = sock.into();
    Ok(s.try_into()?)
}

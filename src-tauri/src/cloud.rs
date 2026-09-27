use crate::lan::LanState;
use crate::protocol::{IncomingMessage, Packet};
use reqwest::Client;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// 云端中继链路：
/// - 发送：POST {relay}/api/send   body: Packet(JSON)
/// - 接收：GET  {relay}/api/poll?device_id=xxx&since=xxx  返回 Packet 数组
/// 中继服务端只做转发与签名透传，密钥不经过云端。
pub fn start(app: AppHandle, state: std::sync::Arc<LanState>) {
    tokio::spawn(cloud_poll_loop(app, state));
}

async fn cloud_poll_loop(app: AppHandle, state: std::sync::Arc<LanState>) {
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .expect("http client");
    let mut since = crate::protocol::now_ms();
    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let (enabled, url, device_id, key) = {
            let c = state.config.read().await;
            (
                c.cloud_enabled,
                c.cloud_relay_url.clone(),
                c.device_id.clone(),
                c.shared_key.clone(),
            )
        };
        if !enabled || url.trim().is_empty() {
            continue;
        }
        let endpoint = format!(
            "{}/api/poll?device_id={}&since={}",
            url.trim_end_matches('/'),
            urlencode(&device_id),
            since
        );
        let resp = client.get(&endpoint).send().await;
        if let Ok(r) = resp {
            if r.status().is_success() {
                if let Ok(packets) = r.json::<Vec<Packet>>().await {
                    for pkt in packets {
                        if pkt.kind != "msg" || !pkt.verify(&key) {
                            continue;
                        }
                        since = since.max(pkt.ts);
                let incoming = IncomingMessage {
                    id: pkt.id.clone(),
                    from: pkt.from.clone(),
                    text: pkt.text.clone(),
                    ts: pkt.ts,
                    via: "cloud".into(),
                };
                let _ = app.emit("bc://incoming", &incoming);
                let app2 = app.clone();
                let st2 = state.clone();
                let inc2 = incoming.clone();
                tauri::async_runtime::spawn(async move {
                    crate::overlay::show(&app2, &st2.config, &inc2).await;
                });
                    }
                }
            }
        }
    }
}

/// 教师端经云端中继下发（可与大屏端局域网投递并行，双链路互补）
pub async fn send_cloud(state: &LanState, from: &str, text: &str) -> Result<(), String> {
    let cfg = state.config.read().await.clone();
    if !cfg.cloud_enabled || cfg.cloud_relay_url.trim().is_empty() {
        return Err("云端链路未启用".into());
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let mut pkt = Packet::new("msg", &cfg.device_id);
    pkt.from = from.into();
    pkt.text = text.into();
    pkt.sign(&cfg.shared_key);
    let endpoint = format!("{}/api/send", cfg.cloud_relay_url.trim_end_matches('/'));
    let resp = client
        .post(&endpoint)
        .json(&pkt)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("中继返回 {}", resp.status()))
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

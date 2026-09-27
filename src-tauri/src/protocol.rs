use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_TEXT_LEN: usize = 2000;
/// announce 有效期：超时视为离线
pub const PEER_TIMEOUT_MS: i64 = 15_000;

type HmacSha256 = Hmac<Sha256>;

/// 班级分组的传输 DTO（与 config::Group 结构一致，避免重复定义依赖）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDto {
    pub id: String,
    pub name: String,
    pub class_ids: Vec<String>,
}

/// 网络包：announce（大屏发现）/ msg（广播消息）/ ack（回执）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub v: u8,
    pub kind: String, // "announce" | "msg" | "ack"
    pub id: String,
    pub device_id: String,
    #[serde(default)]
    pub class_id: String,
    #[serde(default)]
    pub class_name: String,
    #[serde(default)]
    pub classes: Vec<ClassDto>,
    #[serde(default)]
    pub groups: Vec<GroupDto>,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub text: String,
    pub ts: i64,
    pub nonce: String,
    pub sig: String,
    /// 大屏端实际 TCP 端口（announce 携带；同机多实例端口错开）
    #[serde(default)]
    pub tcp_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDto {
    pub id: String,
    pub name: String,
}

impl Packet {
    pub fn new(kind: &str, device_id: &str) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            kind: kind.into(),
            id: uuid::Uuid::new_v4().to_string(),
            device_id: device_id.into(),
            class_id: String::new(),
            class_name: String::new(),
            classes: vec![],
            groups: vec![],
            from: String::new(),
            text: String::new(),
            ts: now_ms(),
            nonce: uuid::Uuid::new_v4().to_string(),
            sig: String::new(),
            tcp_port: 0,
        }
    }

    /// 计算并填充 HMAC-SHA256 签名
    pub fn sign(&mut self, key: &str) {
        let payload = self.signing_payload();
        let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("hmac key");
        mac.update(payload.as_bytes());
        self.sig = hex::encode(mac.finalize().into_bytes());
    }

    /// 校验签名 + 时效（±2 分钟防重放）
    pub fn verify(&self, key: &str) -> bool {
        if self.v != PROTOCOL_VERSION {
            return false;
        }
        let drift = (now_ms() - self.ts).abs();
        if drift > 120_000 {
            return false;
        }
        let payload = self.signing_payload();
        let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("hmac key");
        mac.update(payload.as_bytes());
        let expect = hex::encode(mac.finalize().into_bytes());
        // 常量时间比较
        if expect.len() != self.sig.len() {
            return false;
        }
        expect
            .bytes()
            .zip(self.sig.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }

    fn signing_payload(&self) -> String {
        format!(
            "v{}|{}|{}|{}|{}|{}|{}|{}",
            self.v, self.kind, self.id, self.device_id, self.from, self.text, self.ts, self.nonce
        )
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 收到广播消息后投递给前端的事件负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub id: String,
    pub from: String,
    pub text: String,
    pub ts: i64,
    pub via: String, // "lan" | "cloud"
}

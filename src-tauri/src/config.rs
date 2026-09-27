use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 角色模式：教师端 / 大屏端
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Teacher,
    Screen,
}

/// 提醒样式（四种）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderStyle {
    /// 顶部胶囊提醒
    Capsule,
    /// 全屏划入提醒
    Fullscreen,
    /// CI 接管提醒（ClassIsland 风格）
    ClassIsland,
    /// CW 接管提醒（ClassWidget 风格）
    ClassWidget,
}

impl ReminderStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReminderStyle::Capsule => "capsule",
            ReminderStyle::Fullscreen => "fullscreen",
            ReminderStyle::ClassIsland => "class_island",
            ReminderStyle::ClassWidget => "class_widget",
        }
    }
}

/// 班级信息（大屏端维护，随 announce 广播给教师端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassInfo {
    pub id: String,
    pub name: String,
}

/// 班级分组
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    /// 分组内包含的班级 id
    pub class_ids: Vec<String>,
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub mode: Mode,
    /// 本机设备唯一 id（区分多台大屏）
    pub device_id: String,
    /// 教师端默认发送者显示名称
    pub sender_name: String,
    /// 大屏端班级 id / 名称
    pub class_id: String,
    pub class_name: String,
    /// 班级列表（大屏端维护；一台大屏可声明多个班级）
    pub classes: Vec<ClassInfo>,
    /// 分组列表
    pub groups: Vec<Group>,
    /// 提醒样式
    pub reminder_style: ReminderStyle,
    /// UDP 发现 / TCP 通信 基础端口（TCP = udp + 1）
    pub lan_port: u16,
    /// 共享频道密钥（HMAC-SHA256 签名防伪造）
    pub shared_key: String,
    /// 云端中继地址（跨网段远距离推送，留空则禁用云端链路）
    pub cloud_relay_url: String,
    /// 是否启用云端链路轮询
    pub cloud_enabled: bool,
    /// 设置保护密码（SHA-256 hex；空=未设置。设置后修改配置/退出需验证）
    pub settings_password_hash: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Screen,
            device_id: uuid::Uuid::new_v4().to_string(),
            sender_name: "教师".into(),
            class_id: uuid::Uuid::new_v4().to_string(),
            class_name: "未命名班级".into(),
            classes: vec![],
            groups: vec![],
            reminder_style: ReminderStyle::Capsule,
            lan_port: 23456,
            shared_key: "CB-DEFAULT-CHANNEL-KEY".into(),
            cloud_relay_url: String::new(),
            cloud_enabled: false,
            settings_password_hash: String::new(),
        }
    }
}

impl AppConfig {
    /// 归一化：确保 classes 中至少包含当前主班级
    pub fn normalize(&mut self) {
        if self.classes.is_empty() {
            self.classes.push(ClassInfo {
                id: self.class_id.clone(),
                name: self.class_name.clone(),
            });
        }
        if !self.classes.iter().any(|c| c.id == self.class_id) {
            if let Some(first) = self.classes.first().cloned() {
                self.class_id = first.id.clone();
                self.class_name = first.name.clone();
            }
        }
        self.lan_port = self.lan_port.clamp(1024, 65000);
        if self.shared_key.trim().is_empty() {
            self.shared_key = "CB-DEFAULT-CHANNEL-KEY".into();
        }
    }
}

/// 配置存储：appdata/classbroadcast/config.json
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self {
            path: dir.join("config.json"),
        }
    }

    pub fn load(&self) -> AppConfig {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice::<AppConfig>(&bytes).unwrap_or_default(),
            Err(_) => AppConfig::default(),
        }
    }

    pub fn save(&self, cfg: &AppConfig) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| e.to_string())?;
        // 先写临时文件再原子替换，避免断电损坏配置
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

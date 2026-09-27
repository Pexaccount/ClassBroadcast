/** 与 src-tauri 后端 DTO 保持一致 */

export type Mode = "teacher" | "screen";
export type ReminderStyle = "capsule" | "fullscreen" | "class_island" | "class_widget";

export interface ClassInfo {
  id: string;
  name: string;
}

export interface Group {
  id: string;
  name: string;
  class_ids: string[];
}

export interface AppConfig {
  mode: Mode;
  device_id: string;
  sender_name: string;
  class_id: string;
  class_name: string;
  classes: ClassInfo[];
  groups: Group[];
  reminder_style: ReminderStyle;
  lan_port: number;
  shared_key: string;
  cloud_relay_url: string;
  cloud_enabled: boolean;
  settings_password_hash: string;
}

export interface ClassDto {
  id: string;
  name: string;
}

export interface PeerInfo {
  device_id: string;
  class_id: string;
  class_name: string;
  classes: ClassDto[];
  groups: GroupDto[];
  ip: string;
  last_seen: number;
  tcp_port: number;
}

export interface GroupDto {
  id: string;
  name: string;
  class_ids: string[];
}

export interface SendResult {
  class_id: string;
  class_name: string;
  ok: boolean;
  detail: string;
}

export interface SendReport {
  results: SendResult[];
  cloud_ok: boolean | null;
  cloud_detail: string;
}

export interface IncomingMessage {
  id: string;
  from: string;
  text: string;
  ts: number;
  via: "lan" | "cloud";
}

import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { AppConfig, IncomingMessage, PeerInfo, SendReport, SendResult } from "./types";

export const api = {
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (config: AppConfig, authPassword?: string) =>
    invoke<void>("save_config", { config, authPassword: authPassword ?? null }),
  sendBroadcast: (classIds: string[], senderName: string, text: string) =>
    invoke<SendReport>("send_broadcast", {
      classIds,
      senderName,
      text,
    }),
  listPeers: () => invoke<PeerInfo[]>("list_peers"),
  getInbox: () => invoke<IncomingMessage[]>("get_inbox"),
  getLocalIp: () => invoke<string>("get_local_ip"),
  exitApp: (password?: string) => invoke<void>("exit_app", { password: password ?? null }),
  hasPassword: () => invoke<boolean>("has_password"),
  monitorFrame: (ip: string, port?: number) =>
    invoke<string>("monitor_frame", { ip, port: port ?? null }),
  monitorScreenshot: (ip: string, port?: number) =>
    invoke<string>("monitor_screenshot", { ip, port: port ?? null }),
  monitorClick: (ip: string, port: number | undefined, x: number, y: number) =>
    invoke<void>("monitor_click", { ip, port: port ?? null, x, y }),
  monitorType: (ip: string, port: number | undefined, text: string) =>
    invoke<void>("monitor_type", { ip, port: port ?? null, text }),
  autostartEnable: () => invoke<void>("plugin:autostart|enable"),
  autostartDisable: () => invoke<void>("plugin:autostart|disable"),
  autostartIsEnabled: () => invoke<boolean>("plugin:autostart|is_enabled"),
  onIncoming: (cb: (msg: IncomingMessage) => void): Promise<UnlistenFn> =>
    listen<IncomingMessage>("bc://incoming", (e) => cb(e.payload)),
  onSendResult: (cb: (r: SendResult) => void): Promise<UnlistenFn> =>
    listen<SendResult>("bc://send-result", (e) => cb(e.payload)),
};

export function uid(): string {
  return crypto.randomUUID();
}

export function fmtTime(ts: number): string {
  return new Date(ts).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

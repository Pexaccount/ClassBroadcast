/** 应用入口：角色选择（首次启动）→ 教师端 / 大屏端 */

import { api } from "./api";
import type { AppConfig, Mode } from "./types";
import { ScreenView } from "./screen";
import { TeacherView } from "./teacher";
import { buildTitlebar, el, modal, svgIcon, toast } from "./ui";

const app = document.getElementById("app")!;

/** 全屏提醒覆盖窗口入口（由后端创建，initialization script 打标记） */
async function bootOverlay(): Promise<void> {
  document.body.classList.add("overlay");
  const { listen } = await import("@tauri-apps/api/event");
  const { showReminder } = await import("./reminders");
  await listen<{ style: string; from: string; text: string; duration: number }>("bc://show", (e) => {
    const p = e.payload;
    // 按样式铺底色（窗口不透明方案，虚拟机友好）
    const bg: Record<string, string> = {
      capsule: "#14161a",
      fullscreen: "#14161a",
      class_island: "#eef1f5",
      class_widget: "#eef1f5",
    };
    document.body.style.background = bg[p.style] ?? "#14161a";
    // 清掉上一条残留（A 轮/B 轮问题），再渲染新消息 + 提示音
    const layer = document.querySelector(".reminder-layer");
    if (layer) layer.innerHTML = "";
    try {
      new Audio("sounds/notify.wav").play().catch(() => {});
    } catch { /* 自动播放策略失败时静默 */ }
    showReminder(
      p.style as Parameters<typeof showReminder>[0],
      { id: "ov", from: p.from, text: p.text, ts: Date.now(), via: "lan" },
      p.duration,
    );
  });
}

async function boot() {
  // 覆盖窗口：只做全屏提醒
  if ((window as unknown as { __CB_OVERLAY__?: boolean }).__CB_OVERLAY__) {
    await bootOverlay();
    return;
  }
  let cfg: AppConfig;
  try {
    cfg = await api.getConfig();
  } catch (e) {
    app.innerHTML = `<div style="padding:40px;font-family:sans-serif">配置加载失败：${String(e)}</div>`;
    return;
  }

  // 双 exe 固定角色：不再弹角色选择
  if (!localStorage.getItem("cb_mode_chosen")) {
    await chooseMode(cfg);
  }

  renderApp(cfg);
}

function chooseMode(cfg: AppConfig): Promise<void> {
  return new Promise((resolve) => {
    const body = el("div", {}, [
      el("p", { style: "color:var(--text-2);margin-bottom:16px;line-height:1.7;" },
        ["请选择本机在本校 Class Broadcast 系统中的角色。角色后续可在「设置」中切换。"]),
      el("div", { style: "display:flex;gap:12px;" }, [
        el("button", { class: "btn primary", style: "flex:1;padding:18px;", onclick: () => pick("teacher") },
          ["教师端\n批量向多班级发送喊话消息"]),
        el("button", { class: "btn", style: "flex:1;padding:18px;", onclick: () => pick("screen") },
          ["大屏管理端\n维护班级分组并接收广播"]),
      ]),
    ]);
    const m = modal("欢迎使用 Class Broadcast", body, []);
    function pick(mode: Mode) {
      cfg.mode = mode;
      localStorage.setItem("cb_mode_chosen", "1");
      void api.saveConfig(cfg);
      m.remove();
      resolve();
    }
  });
}

function renderApp(cfg: AppConfig) {
  app.innerHTML = "";
  const titlebar = buildTitlebar(cfg.mode);
  const layout = el("div", { class: "layout" });
  const sidenav = el("nav", { class: "sidenav" });

  // 侧栏：当前视图 + 设置入口（角色切换在「设置」中，受保护密码约束）
  const view = cfg.mode === "teacher" ? new TeacherView(cfg) : new ScreenView(cfg);
  const navCurrent = el("button", {
    class: "nav-item active",
    onclick: () => toast("已位于当前页", "info"),
  }, [
    svgIcon(cfg.mode === "teacher" ? "mail" : "monitor"),
    cfg.mode === "teacher" ? "发送广播" : "管理与接收",
  ]);

  const settingsBtn = el("button", { class: "nav-item", onclick: () => openSettings(cfg) }, [svgIcon("cog"), "设置"]);
  sidenav.append(navCurrent);
  sidenav.append(settingsBtn);
  sidenav.append(
    el("div", { class: "footnote" }, [
      "Class Broadcast v1.0.0",
      el("br"),
      "基于 ClassIsland 生态 · 双链路传输",
      el("br"),
      "© 新启年工作室",
    ])
  );

  layout.append(sidenav, view.node);
  app.append(titlebar, layout);

  // 订阅广播消息（大屏端展示提醒）
  void api.onIncoming((msg) => {
    if (view instanceof ScreenView) {
      view.onIncoming(msg);
    }
  });

  // 托盘退出请求（受保护时要求密码）
  void import("@tauri-apps/api/event").then(({ listen }) =>
    listen("bc://quit-request", () => void handleQuitRequest()),
  );
}

function openSettings(cfg: AppConfig) {
  const keyInput = el("input", { class: "input", value: cfg.shared_key }) as HTMLInputElement;
  const portInput = el("input", { class: "input", type: "number", value: String(cfg.lan_port) }) as HTMLInputElement;
  const senderInput = el("input", { class: "input", value: cfg.sender_name }) as HTMLInputElement;
  const cloudToggle = el("input", { type: "checkbox" }) as HTMLInputElement;
  cloudToggle.checked = cfg.cloud_enabled;
  const cloudInput = el("input", {
    class: "input",
    value: cfg.cloud_relay_url,
    placeholder: "https://your-relay.example.com（跨网段中继服务地址）",
  }) as HTMLInputElement;

  // 提醒样式（四种）
  const styleNames: Record<string, string> = {
    capsule: "顶部胶囊提醒",
    fullscreen: "全屏划入提醒",
    class_island: "CI 接管提醒",
    class_widget: "CW 接管提醒",
  };
  const styleSel = el("select", { class: "select" }) as HTMLSelectElement;
  for (const [k, name] of Object.entries(styleNames)) {
    const o = el("option", { value: k }, [name]) as HTMLOptionElement;
    if (cfg.reminder_style === k) o.selected = true;
    styleSel.append(o);
  }

  // 角色（教师端 / 大屏端）
  const roleSel = el("select", { class: "select" }) as HTMLSelectElement;
  roleSel.append(el("option", { value: "teacher" }, ["教师端（发送广播）"]) as HTMLOptionElement);
  roleSel.append(el("option", { value: "screen" }, ["大屏管理端（接收广播）"]) as HTMLOptionElement);
  roleSel.value = cfg.mode;

  // 开机自启动
  const autoToggle = el("input", { type: "checkbox" }) as HTMLInputElement;
  void api.autostartIsEnabled().then((on) => (autoToggle.checked = !!on)).catch(() => {});

  // 保护密码（SHA-256 前端散列，后端只存哈希）
  const authWrap = el("div", { class: "field" });
  const authInput = el("input", { class: "input", type: "password", placeholder: "输入当前保护密码" }) as HTMLInputElement;
  const newPw = el("input", { class: "input", type: "password", placeholder: "设置保护密码（留空=不更改）" }) as HTMLInputElement;
  const newPw2 = el("input", { class: "input", type: "password", placeholder: "再次输入新保护密码" }) as HTMLInputElement;
  authWrap.append(newPw, newPw2);
  void api.hasPassword().then((has) => {
    if (has) authWrap.prepend(authInput);
  });

  async function sha256Hex(s: string): Promise<string> {
    const buf = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s));
    return Array.from(new Uint8Array(buf)).map((b) => b.toString(16).padStart(2, "0")).join("");
  }

  const body = el("div", {}, [
    el("div", { class: "field" }, [el("label", {}, ["本机角色（教师端 / 大屏端）"]), roleSel]),
    el("div", { class: "field" }, [el("label", {}, ["广播提醒样式（收到消息时全屏弹出）"]), styleSel]),
    el("div", { class: "field" }, [el("label", {}, ["频道密钥（两端必须一致，用于消息签名防伪造）"]), keyInput]),
    el("div", { class: "field" }, [el("label", {}, ["局域网端口（TCP 使用 端口+1）"]), portInput]),
    el("div", { class: "field" }, [el("label", {}, ["教师端默认发送者名称"]), senderInput]),
    el("div", { class: "field" }, [
      el("label", {}, [cloudToggle, " 启用云端链路（跨网段远距离推送）"]),
      cloudInput,
    ]),
    el("div", { class: "field" }, [
      el("label", {}, [autoToggle, " 开机自启动（大屏端建议开启）"]),
    ]),
    el("div", { class: "field" }, [
      el("label", {}, ["保护密码（设置后：修改设置、托盘退出均需验证，防学生乱改）"]),
      authWrap,
    ]),
    el("p", { style: "font-size:12px;color:var(--text-3);line-height:1.7;" },
      ["局域网模式无需任何服务器，同一校园网内自动发现、开箱即用；", el("br"),
       "关闭窗口即最小化到托盘；云端链路仅在跨网段时使用（消息本地签名，密钥不出本机）。"]),
  ]);

  const m = modal("设置", body, [
    el("button", { class: "btn", onclick: () => m.remove() }, ["取消"]),
    el("button", {
      class: "btn primary",
      onclick: async () => {
        // 密码校验/设置
        let authPw: string | undefined;
        if (authWrap.contains(authInput) && authInput.value) authPw = authInput.value;
        const np = newPw.value, np2 = newPw2.value;
        if (np || np2) {
          if (np !== np2) return void toast("两次输入的新密码不一致", "err");
          if (np.length < 4) return void toast("密码至少 4 位", "err");
          cfg.settings_password_hash = await sha256Hex(np);
        }
        cfg.shared_key = keyInput.value.trim() || cfg.shared_key;
        cfg.lan_port = Math.max(1024, Math.min(65000, Number(portInput.value) || cfg.lan_port));
        cfg.sender_name = senderInput.value.trim() || "教师";
        cfg.cloud_enabled = cloudToggle.checked;
        cfg.cloud_relay_url = cloudInput.value.trim();
        cfg.reminder_style = styleSel.value as AppConfig["reminder_style"];
        const roleChanged = cfg.mode !== (roleSel.value as AppConfig["mode"]);
        cfg.mode = roleSel.value as AppConfig["mode"];
        try {
          await api.saveConfig(cfg, authPw);
        } catch (e) {
          return void toast(`保存失败：${e}`, "err");
        }
        // 自启动
        try {
          if (autoToggle.checked) await api.autostartEnable();
          else await api.autostartDisable();
        } catch { toast("自启动设置失败（权限不足）", "err"); }
        toast("设置已保存，部分改动需重启应用生效", "ok");
        m.remove();
        if (roleChanged) renderApp(cfg); // 角色切换立即生效
      },
    }, ["保存"]),
  ]);
}

/** 托盘退出：受保护时要求输入密码 */
async function handleQuitRequest() {
  const win = (await import("@tauri-apps/api/window")).getCurrentWindow();
  void win.show();
  void win.setFocus();
  try {
    if (await api.hasPassword()) {
      const input = el("input", { class: "input", type: "password", placeholder: "输入保护密码以退出" }) as HTMLInputElement;
      const m = modal("退出验证", el("div", { class: "field" }, [input]), [
        el("button", { class: "btn", onclick: () => m.remove() }, ["取消"]),
        el("button", {
          class: "btn primary",
          onclick: () => {
            void api.exitApp(input.value);
            m.remove();
          },
        }, ["确认退出"]),
      ]);
      setTimeout(() => input.focus(), 50);
    } else {
      await api.exitApp();
    }
  } catch {
    await api.exitApp();
  }
}

void boot();

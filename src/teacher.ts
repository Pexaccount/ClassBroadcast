/** 教师端：批量班级选择 + 自定义发送者 + 广播发送 + 历史记录 */

import { api, fmtTime } from "./api";
import type { AppConfig, PeerInfo, SendResult } from "./types";
import { el, svgIcon, toast } from "./ui";

interface HistoryItem {
  ts: number;
  from: string;
  text: string;
  results: SendResult[];
}

export class TeacherView {
  private static monSeq = 0;
  private cfg: AppConfig;
  private peers: PeerInfo[] = [];
  private selected = new Set<string>();
  private history: HistoryItem[] = [];
  private root: HTMLElement;

  constructor(cfg: AppConfig) {
    this.cfg = cfg;
    this.root = el("div", { class: "content" });
    this.render();
    this.refreshPeers();
    setInterval(() => this.refreshPeers(), 2500);
    // 节点变化实时刷新（后端 emit）
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen("bc://peers-changed", () => void this.refreshPeers()),
    );
    // 后台投递结果实时回显
    void api.onSendResult((r) => this.applySendResult(r));
  }

  private applySendResult(r: SendResult) {
    const row = this.root.querySelector(`.send-result .row[data-cid="${r.class_id}"]`);
    if (row) {
      const badge = row.querySelector(".badge");
      if (badge) {
        badge.className = `badge ${r.ok ? "ok" : "err"}`;
        badge.textContent = r.detail;
      }
    }
    // 同步最近一条历史
    for (let i = 0; i < this.history.length && i < 5; i++) {
      const h = this.history[i];
      const hit = h.results.find((x) => x.class_id === r.class_id);
      if (hit) {
        hit.ok = r.ok;
        hit.detail = r.detail;
        this.renderHistory();
        break;
      }
    }
  }

  get node(): HTMLElement {
    return this.root;
  }

  private async refreshPeers() {
    try {
      this.peers = await api.listPeers();
      this.renderTargets();
      this.updateStats();
    } catch {
      /* 忽略轮询错误 */
    }
  }

  /** 所有可发送的班级（来自各在线大屏 announce） */
  private allClasses(): { id: string; name: string; online: boolean; groups: string[] }[] {
    const map = new Map<string, { id: string; name: string; online: boolean; groups: string[] }>();
    for (const peer of this.peers) {
      const gNames = peer.groups ?? [];
      for (const c of peer.classes ?? []) {
        const gs = gNames
          .filter((g) => g.class_ids.includes(c.id))
          .map((g) => g.name);
        const prev = map.get(c.id);
        map.set(c.id, {
          id: c.id,
          name: c.name,
          online: true,
          groups: gs.length ? gs : prev?.groups ?? [],
        });
      }
    }
    // 教师端配置中缓存的班级（离线也显示，标记离线）
    for (const c of this.cfg.classes) {
      if (!map.has(c.id)) {
        map.set(c.id, { id: c.id, name: c.name, online: false, groups: [] });
      }
    }
    return Array.from(map.values()).sort((a, b) => Number(b.online) - Number(a.online));
  }

  private render() {
    this.root.innerHTML = "";

    // 状态卡
    const stats = el("div", { class: "stat-row" });
    stats.append(
      this.statEl("targets", "在线大屏"),
      this.statEl("classes", "可发送班级"),
      this.statEl("selected", "已勾选"),
    );
    this.root.append(stats);

    // 发送区
    const sendCard = el("div", { class: "card" }, [
      el("h3", {}, [svgIcon("send"), "发送广播", el("span", { class: "hint" }, ["支持一次勾选多个班级批量下发"])]),
    ]);

    // 分组快捷行 + 班级网格
    const groupBar = el("div", { class: "group-row", id: "group-bar" });
    const grid = el("div", { class: "class-grid", id: "class-grid" });
    const targetCard = el("div", { class: "card" }, [
      el("h3", {}, [svgIcon("screen"), "目标班级", el("span", { class: "hint", id: "peer-hint" })]),
      groupBar,
      grid,
    ]);

    // 发件人 + 内容
    const senderInput = el("input", {
      class: "input",
      value: this.cfg.sender_name,
      placeholder: "消息中显示的发送者名称（可不填，默认使用配置）",
    }) as HTMLInputElement;
    const textarea = el("textarea", {
      class: "textarea",
      placeholder: "输入要下发的广播内容，例如：「第三节体育课改到风雨操场集合」",
      maxlength: "2000",
    }) as HTMLTextAreaElement;
    const counter = el("span", { class: "hint" }, ["0 / 2000"]);
    textarea.addEventListener("input", () => {
      counter.textContent = `${textarea.value.length} / 2000`;
    });

    const sendBtn = el("button", { class: "btn primary" }, [svgIcon("send"), "立即发送"]) as HTMLButtonElement;
    const resultBox = el("div", { class: "send-result" });

    sendBtn.addEventListener("click", async () => {
      const text = textarea.value.trim();
      if (!text) return void toast("请输入广播内容", "err");
      if (this.selected.size === 0) return void toast("请先勾选目标班级", "err");
      sendBtn.disabled = true;
      sendBtn.textContent = "发送中…";
      try {
        const report = await api.sendBroadcast(Array.from(this.selected), senderInput.value, text);
        resultBox.innerHTML = "";
        for (const r of report.results) {
          resultBox.append(
            el("div", { class: "row", "data-cid": r.class_id }, [
              el("span", { class: "name" }, [r.class_name]),
              el("span", { class: `badge ${r.ok ? "ok" : "err"}` }, [r.detail]),
            ])
          );
        }
        if (report.results.length === 0) {
          resultBox.append(
            el("div", { class: "row" }, [svgIcon("warn", 14), "未匹配到任何在线大屏，消息未送达"])
          );
        }
        this.history.unshift({ ts: Date.now(), from: senderInput.value || this.cfg.sender_name, text, results: report.results });
        this.renderHistory();
        toast("已加入投递队列，后台自动重试", "ok");
      } catch (e) {
        toast(`发送失败：${e}`, "err");
      } finally {
        sendBtn.disabled = false;
        sendBtn.innerHTML = "";
        sendBtn.append(svgIcon("send"), "立即发送");
      }
    });

    sendCard.append(
      el("div", { class: "field" }, [el("label", {}, ["发送者名称"]), senderInput]),
      el("div", { class: "field" }, [
        el("label", {}, [el("span", {}, ["广播内容"]), counter]),
        textarea,
      ]),
      sendBtn,
      resultBox,
    );

    // —— 远程监控（默认开启，不可关闭）——
    const monSel = el("select", { class: "select" }) as HTMLSelectElement;
    const monView = el("img", {
      class: "monitor-view",
      alt: "实时监控",
      style: "display:block;width:100%;border-radius:8px;border:1px solid var(--border);background:#000;aspect-ratio:16/9;object-fit:contain;cursor:crosshair;",
    }) as HTMLImageElement;
    let recording = false;
    let recFrames: Uint8Array[] = [];
    let monToken = ++TeacherView.monSeq; // 本视图会话标记：视图重建后旧循环自停

    const renderMonTargets = () => {
      monSel.innerHTML = "";
      const seen = new Map<string, string>(); // "ip|port" -> label
      for (const p of this.peers) {
        for (const c of p.classes ?? []) {
          const key = `${p.ip}|${p.tcp_port ?? 0}`;
          if (!seen.has(key)) seen.set(key, `${c.name}（${p.ip}）`);
        }
      }
      if (seen.size === 0) {
        monSel.append(el("option", { value: "" }, ["暂无在线大屏"]));
        return;
      }
      for (const [key, label] of seen) {
        monSel.append(el("option", { value: key }, [label]));
      }
      // 默认锁定第一个在线端点
      if (!seen.has(monSel.value)) monSel.value = seen.keys().next().value as string;
    };
    renderMonTargets();

    const parseSel = (): [string, number | undefined] | null => {
      const [ip, portStr] = (monSel.value || "").split("|");
      if (!ip) return null;
      return [ip, Number(portStr) || undefined];
    };

    // 连续拉帧循环：一帧回来立刻请求下一帧（尽力逼近 60fps）
    const loop = async () => {
      while (monToken === TeacherView.monSeq) {
        const t = parseSel();
        if (!t) {
          await new Promise((r) => setTimeout(r, 500));
          continue;
        }
        try {
          const b64 = await api.monitorFrame(t[0], t[1]);
          monView.src = `data:image/jpeg;base64,${b64}`;
          if (recording) {
            const bin = atob(b64);
            const arr = new Uint8Array(bin.length);
            for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
            recFrames.push(arr);
          }
        } catch {
          await new Promise((r) => setTimeout(r, 800)); // 对端暂不可达，稍后重试
        }
      }
    };
    void loop();

    // 远程操作：点击画面 = 点击大屏对应位置
    monView.addEventListener("click", (ev) => {
      const t = parseSel();
      if (!t || !monView.naturalWidth) return void toast("暂无在线大屏", "err");
      const rect = monView.getBoundingClientRect();
      const me = ev as MouseEvent;
      const nx = (me.clientX - rect.left) / rect.width;
      const ny = (me.clientY - rect.top) / rect.height;
      const absX = Math.round(nx * monView.naturalWidth);
      const absY = Math.round(ny * monView.naturalHeight);
      void api.monitorClick(t[0], t[1], absX, absY).catch((e) => toast(`操作失败：${e}`, "err"));
    });

    // 远程键盘输入
    const typeInput = el("input", { class: "input", placeholder: "输入文字远程输入到目标屏幕" }) as HTMLInputElement;
    const typeBtn = el("button", { class: "btn sm" }, ["发送输入"]);
    typeBtn.addEventListener("click", () => {
      const t = parseSel();
      const s = typeInput.value;
      if (!t) return void toast("暂无在线大屏", "err");
      if (!s) return;
      void api.monitorType(t[0], t[1], s).then(() => { typeInput.value = ""; }).catch((e) => toast(`输入失败：${e}`, "err"));
    });

    const shotBtn = el("button", { class: "btn sm" }, ["截图"]);
    shotBtn.addEventListener("click", () => {
      const t = parseSel();
      if (!t) return void toast("暂无在线大屏", "err");
      void api.monitorScreenshot(t[0], t[1]).then((p) => toast(`截图已保存：${p}`, "ok")).catch((e) => toast(`截图失败：${e}`, "err"));
    });
    const recBtn = el("button", { class: "btn sm" }) as HTMLButtonElement;
    recBtn.textContent = "开始录像";
    recBtn.addEventListener("click", () => {
      if (!recording) {
        recording = true;
        recFrames = [];
        recBtn.textContent = "保存录像并停止";
        return;
      }
      recording = false;
      recBtn.textContent = "开始录像";
      if (recFrames.length === 0) return void toast("未捕获到任何帧", "err");
      // MJPEG（VLC / PotPlayer 可直接播放）
      const blob = new Blob(recFrames as BlobPart[], { type: "image/jpeg" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = `CB-录像-${Date.now()}.mjpeg`;
      a.click();
      URL.revokeObjectURL(a.href);
      toast(`录像已保存（${recFrames.length} 帧，MJPEG 格式）`, "ok");
    });

    const monitorCard = el("div", { class: "card" }, [
      el("h3", {}, [svgIcon("monitor"), "实时监控", el("span", { class: "hint" }, ["默认开启 · 高清实时 · 点击画面可远程操作"])]),
      el("div", { class: "field" }, [monSel]),
      monView,
      el("div", { style: "display:flex;gap:8px;flex-wrap:wrap;margin-top:10px;" }, [typeInput, typeBtn, shotBtn, recBtn]),
    ]);

    const historyCard = el("div", { class: "card" }, [
      el("h3", {}, [svgIcon("inbox"), "发送记录", el("span", { class: "hint" }, ["本端保留最近 50 条"])]),
      el("div", { id: "history-list" }, [el("div", { class: "empty" }, ["暂无发送记录"])]),
    ]);
    this.historyEl = historyCard.querySelector("#history-list") as HTMLElement;

    this.root.append(targetCard, sendCard, monitorCard, historyCard);
    this.renderTargets();
    // 监控目标随在线状态刷新
    const origRefresh = this.refreshPeers.bind(this);
    this.refreshPeers = async () => {
      await origRefresh();
      renderMonTargets();
    };
  }

  private historyEl!: HTMLElement;

  private statEl(id: string, label: string): HTMLElement {
    return el("div", { class: "stat" }, [
      el("div", { class: "v", id: `stat-${id}` }, ["0"]),
      el("div", { class: "k" }, [label]),
    ]);
  }

  private updateStats() {
    const set = (id: string, v: string) => {
      const n = this.root.querySelector(`#stat-${id}`);
      if (n) n.textContent = v;
    };
    set("targets", String(new Set(this.peers.map((p) => p.device_id)).size));
    set("classes", String(this.allClasses().length));
    set("selected", String(this.selected.size));
  }

  private renderTargets() {
    const grid = this.root.querySelector("#class-grid");
    if (!grid) return;
    grid.innerHTML = "";
    const classes = this.allClasses();
    const hint = this.root.querySelector("#peer-hint");
    if (hint) hint.textContent = `${this.peers.length} 台大屏在线 · 班级按分组归档`;

    // 分组快捷全选
    const bar = this.root.querySelector("#group-bar") as HTMLElement;
    bar.innerHTML = "";
    const gnames = new Map<string, string[]>();
    for (const p of this.peers) {
      for (const g of p.groups ?? []) {
        gnames.set(g.name, g.class_ids);
      }
    }
    if (gnames.size > 0) {
      bar.append(el("span", { class: "g-head" }, ["按分组快速选择："]));
      const wrap = el("div", { class: "class-grid" });
      for (const [name, ids] of gnames) {
        wrap.append(
          el("span", {
            class: "class-chip",
            onclick: () => {
              ids.forEach((id) => this.selected.add(id));
              this.renderTargets();
              this.updateStats();
            },
          }, [`▸ ${name} (${ids.length})`])
        );
      }
      bar.append(wrap);
    }

    if (classes.length === 0) {
      grid.append(el("div", { class: "empty" }, [
        "尚未发现任何在线大屏。",
        el("br"),
        "请确认：① 大屏管理端已在本校园网内启动；② 两端使用相同端口与频道密钥。",
      ]));
      this.updateStats();
      return;
    }

    for (const c of classes) {
      const chip = el("span", {
        class: `class-chip ${this.selected.has(c.id) ? "on" : ""} ${c.online ? "" : "offline"}`,
        onclick: () => {
          if (this.selected.has(c.id)) this.selected.delete(c.id);
          else this.selected.add(c.id);
          this.renderTargets();
          this.updateStats();
        },
      }, [
        el("span", {}, [c.name]),
        c.groups.length ? el("span", { class: "grp-tag" }, [c.groups.join("/")]) : null,
      ]);
      grid.append(chip);
    }
    this.updateStats();
  }

  private renderHistory() {
    this.historyEl.innerHTML = "";
    if (this.history.length === 0) return;
    for (const h of this.history.slice(0, 50)) {
      const okCount = h.results.filter((r) => r.ok).length;
      this.historyEl.append(
        el("div", { class: "msg-item" }, [
          el("div", { class: "meta" }, [
            el("span", { class: "from" }, [h.from]),
            el("span", {}, [fmtTime(h.ts)]),
            el("span", {}, [`送达 ${okCount}/${h.results.length}`]),
          ]),
          el("div", { class: "body" }, [h.text]),
        ])
      );
    }
  }
}

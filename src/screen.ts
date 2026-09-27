/** 大屏管理端：班级/分组维护、提醒样式选择、广播接收展示、收件箱 */

import { api, fmtTime, uid } from "./api";
import type { AppConfig, ClassInfo, Group, IncomingMessage } from "./types";
import { el, modal, svgIcon, toast } from "./ui";

export class ScreenView {
  private cfg: AppConfig;
  private root: HTMLElement;
  private inbox: IncomingMessage[] = [];

  constructor(cfg: AppConfig) {
    this.cfg = cfg;
    this.root = el("div", { class: "content" });
    this.render();
    void this.loadInbox();
  }

  get node(): HTMLElement {
    return this.root;
  }

  private async loadInbox() {
    try {
      this.inbox = await api.getInbox();
      this.renderInbox();
    } catch {
      /* ignore */
    }
  }

  onIncoming(msg: IncomingMessage) {
    this.inbox.unshift(msg);
    this.renderInbox();
  }

  private async save() {
    try {
      await api.saveConfig(this.cfg);
      toast("配置已保存", "ok");
    } catch (e) {
      toast(`保存失败：${e}`, "err");
    }
  }

  private render() {
    this.root.innerHTML = "";

    // —— 班级管理 ——
    const classList = el("div");
    const renderClasses = () => {
      classList.innerHTML = "";
      if (this.cfg.classes.length === 0) {
        classList.append(el("div", { class: "empty" }, ["暂未添加班级"]));
      }
      for (const c of this.cfg.classes) {
        const groups = this.cfg.groups
          .filter((g) => g.class_ids.includes(c.id))
          .map((g) => g.name)
          .join("、");
        classList.append(
          el("div", { class: "list-row" }, [
            el("div", { class: "grow" }, [
              el("div", {}, [c.name, c.id === this.cfg.class_id ? el("span", { class: "badge info", style: "margin-left:8px" }, ["本机主班级"]) : null]),
              groups ? el("div", { class: "sub" }, [`所属分组：${groups}`]) : null,
            ]),
            el("button", { class: "icon-btn", title: "重命名", onclick: () => this.renameClass(c, renderClasses) }, [svgIcon("pencil", 14)]),
            el("button", { class: "icon-btn danger", title: "删除", onclick: () => this.removeClass(c, renderClasses) }, [svgIcon("trash", 14)]),
          ])
        );
      }
    };
    renderClasses();

    const classCard = el("div", { class: "card" }, [
      el("h3", {}, [svgIcon("screen"), "班级管理", el("span", { class: "hint" }, ["班级会自动广播给局域网内的教师端"])]),
      classList,
      el("button", {
        class: "btn sm",
        onclick: () => this.addClass(renderClasses),
      }, [svgIcon("plus", 13), "添加班级"]),
    ]);

    // —— 分组管理 ——
    const groupList = el("div");
    const renderGroups = () => {
      groupList.innerHTML = "";
      if (this.cfg.groups.length === 0) {
        groupList.append(el("div", { class: "empty" }, ["暂无分组，创建分组后教师端可一键整组勾选"]));
      }
      for (const g of this.cfg.groups) {
        const names = g.class_ids
          .map((id) => this.cfg.classes.find((c) => c.id === id)?.name)
          .filter(Boolean)
          .join("、");
        groupList.append(
          el("div", { class: "list-row" }, [
            el("div", { class: "grow" }, [
              el("div", {}, [g.name]),
              el("div", { class: "sub" }, [names || "空分组"]),
            ]),
            el("button", { class: "icon-btn", title: "编辑", onclick: () => this.editGroup(g, renderGroups) }, [svgIcon("pencil", 14)]),
            el("button", {
              class: "icon-btn danger",
              title: "删除",
              onclick: () => {
                this.cfg.groups = this.cfg.groups.filter((x) => x.id !== g.id);
                renderGroups();
                void this.save();
              },
            }, [svgIcon("trash", 14)]),
          ])
        );
      }
    };
    renderGroups();

    const groupCard = el("div", { class: "card" }, [
      el("h3", {}, [svgIcon("folder"), "分组管理", el("span", { class: "hint" }, ["如「高一（1）部」「实验班」等，便于批量发送"])]),
      groupList,
      el("button", { class: "btn sm", onclick: () => this.editGroup(null, renderGroups) }, [svgIcon("plus", 13), "创建分组"]),
    ]);

    // —— 收件箱 ——（提醒由全屏覆盖层弹出，此处仅留档）
    this.inboxEl = el("div");
    const inboxCard = el("div", { class: "card" }, [
      el("h3", {}, [svgIcon("inbox"), "收到的广播", el("span", { class: "hint" }, ["双链路消息均在此留档；提醒样式在「设置」中配置"])]),
      this.inboxEl,
    ]);

    this.root.append(classCard, groupCard, inboxCard);
    this.renderInbox();
  }

  private inboxEl!: HTMLElement;

  private renderInbox() {
    if (!this.inboxEl) return;
    this.inboxEl.innerHTML = "";
    if (this.inbox.length === 0) {
      this.inboxEl.append(el("div", { class: "empty" }, ["尚未收到广播消息"]));
      return;
    }
    for (const m of this.inbox.slice(0, 100)) {
      this.inboxEl.append(
        el("div", { class: "msg-item" }, [
          el("div", { class: "meta" }, [
            el("span", { class: "from" }, [m.from]),
            el("span", {}, [fmtTime(m.ts)]),
            el("span", { class: `badge ${m.via === "lan" ? "ok" : "info"}` }, [m.via === "lan" ? "局域网" : "云端"]),
          ]),
          el("div", { class: "body" }, [m.text]),
        ])
      );
    }
  }

  private addClass(rerender: () => void) {
    const input = el("input", { class: "input", placeholder: "输入班级名称，如：高一（3）班" }) as HTMLInputElement;
    const m = modal("添加班级", el("div", { class: "field" }, [input]), [
      el("button", { class: "btn", onclick: () => m.remove() }, ["取消"]),
      el("button", {
        class: "btn primary",
        onclick: () => {
          const name = input.value.trim();
          if (!name) return void toast("请输入班级名称", "err");
          const c: ClassInfo = { id: uid(), name };
          this.cfg.classes.push(c);
          rerender();
          m.remove();
          void this.save();
        },
      }, ["添加"]),
    ]);
    setTimeout(() => input.focus(), 50);
  }

  private renameClass(c: ClassInfo, rerender: () => void) {
    const input = el("input", { class: "input", value: c.name }) as HTMLInputElement;
    const m = modal("重命名班级", el("div", { class: "field" }, [input]), [
      el("button", { class: "btn", onclick: () => m.remove() }, ["取消"]),
      el("button", {
        class: "btn primary",
        onclick: () => {
          const name = input.value.trim();
          if (!name) return;
          c.name = name;
          if (c.id === this.cfg.class_id) this.cfg.class_name = name;
          rerender();
          m.remove();
          void this.save();
        },
      }, ["保存"]),
    ]);
    setTimeout(() => input.focus(), 50);
  }

  private removeClass(c: ClassInfo, rerender: () => void) {
    this.cfg.classes = this.cfg.classes.filter((x) => x.id !== c.id);
    for (const g of this.cfg.groups) {
      g.class_ids = g.class_ids.filter((id) => id !== c.id);
    }
    rerender();
    void this.save();
  }

  private editGroup(g: Group | null, rerender: () => void) {
    const nameInput = el("input", { class: "input", value: g?.name ?? "", placeholder: "分组名称" }) as HTMLInputElement;
    const checks = el("div", { style: "display:flex;flex-direction:column;gap:6px;margin-top:8px;" });
    for (const c of this.cfg.classes) {
      const cb = el("input", { type: "checkbox" }) as HTMLInputElement;
      if (g?.class_ids.includes(c.id)) cb.checked = true;
      cb.dataset.cid = c.id;
      checks.append(el("label", { style: "display:flex;gap:8px;align-items:center;font-size:13px;" }, [cb, c.name]));
    }
    const body = el("div", {}, [
      el("div", { class: "field" }, [el("label", {}, ["分组名称"]), nameInput]),
      el("div", { class: "field" }, [el("label", {}, ["包含班级"]), checks]),
    ]);
    const m = modal(g ? "编辑分组" : "创建分组", body, [
      el("button", { class: "btn", onclick: () => m.remove() }, ["取消"]),
      el("button", {
        class: "btn primary",
        onclick: () => {
          const name = nameInput.value.trim();
          if (!name) return void toast("请输入分组名称", "err");
          const ids = Array.from(checks.querySelectorAll("input:checked")).map(
            (i) => (i as HTMLInputElement).dataset.cid!
          );
          if (g) {
            g.name = name;
            g.class_ids = ids;
          } else {
            this.cfg.groups.push({ id: uid(), name, class_ids: ids });
          }
          rerender();
          m.remove();
          void this.save();
        },
      }, ["保存"]),
    ]);
    setTimeout(() => nameInput.focus(), 50);
  }
}

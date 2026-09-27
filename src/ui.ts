/** 通用 UI 工具：标题栏、toast、模态框 */

import { getCurrentWindow } from "@tauri-apps/api/window";

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string | boolean | ((e: Event) => void)> = {},
  children: (Node | string | null | undefined)[] = []
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (typeof v === "function") {
      node.addEventListener(k.replace(/^on/, ""), v as EventListener);
    } else if (k === "class") {
      node.className = v as string;
    } else if (typeof v === "boolean") {
      (node as unknown as Record<string, unknown>)[k] = v;
    } else {
      node.setAttribute(k, v);
    }
  }
  for (const c of children) {
    if (c == null) continue;
    node.append(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return node;
}

export function svgIcon(name: string, size = 16): SVGSVGElement {
  const paths: Record<string, string> = {
    bell: 'M12 3a6 6 0 0 0-6 6v3.3l-1.6 3A1 1 0 0 0 5.3 17h13.4a1 1 0 0 0 .9-1.7L18 12.3V9a6 6 0 0 0-6-6Zm-2 15a2 2 0 1 0 4 0h-4Z',
    send: 'M3.4 20.4 21 12 3.4 3.6l.03 6.53L15 12 3.43 13.87z',
    screen: 'M3 4h18a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-7v2h3v2H7v-2h3v-2H3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Z',
    cog: 'M12 8.5A3.5 3.5 0 1 0 12 15.5 3.5 3.5 0 0 0 12 8.5Zm9 3.5a8.9 8.9 0 0 0-.1-1.3l2-1.6-2-3.4-2.4 1a9 9 0 0 0-2.2-1.3L16 2.7h-4l-.4 2.7a9 9 0 0 0-2.2 1.3l-2.4-1-2 3.4 2 1.6A8.9 8.9 0 0 0 7 12c0 .4 0 .9.1 1.3l-2 1.6 2 3.4 2.4-1c.7.5 1.4 1 2.2 1.3l.4 2.7h4l.4-2.7a9 9 0 0 0 2.2-1.3l2.4 1 2-3.4-2-1.6c.06-.43.1-.87.1-1.3Z',
    inbox: 'M4 4h16a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Zm1 11v3h14v-3h-3.4a3.6 3.6 0 0 1-7.2 0H5Zm14-2V6H5v7h3a2.5 2.5 0 0 0 5 0h6Z',
    mail: 'M3 5h18a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Zm9 7.5L4.2 7h15.6L12 12.5ZM4 9.2V17h16V9.2l-8 5.5-8-5.5Z',
    monitor: 'M3 4h18a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-7v2h3v2H7v-2h3v-2H3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Zm1 2v9h16V6H4Z',
    pencil: 'M14.1 3.7 20.3 9.9 8.9 21.3H2.7v-6.2L14.1 3.7Zm0 2.8L4.7 15.9v3.4h3.4L18.9 9.9l-4.8-4.8v1.4Z',
    trash: 'M9 3h6l1 2h4v2H4V5h4l1-2Zm-4 6h14l-1 12a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1L5 9Zm4 2 .5 9h1.9l-.5-9H9Zm3.1 0-.5 9h1.9l.5-9h-1.9Z',
    plus: 'M11 5h2v6h6v2h-6v6h-2v-6H5v-2h6V5Z',
    warn: 'M12 2 1 21h22L12 2Zm0 5.5L19.5 19h-15L12 7.5ZM11 10v5h2v-5h-2Zm0 6v2h2v-2h-2Z',
    folder: 'M3 5h6l2 2h10a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Zm1 4v9h16V9H4Z',
    chevron: 'M9 5.3 15.7 12 9 18.7 7.6 17.3 12.9 12 7.6 6.7 9 5.3Z',
  };
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("fill", "currentColor");
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", paths[name] ?? paths.bell);
  svg.append(p);
  return svg;
}

export function toast(text: string, kind: "ok" | "err" | "info" = "info") {
  let wrap = document.querySelector(".toast-wrap") as HTMLElement | null;
  if (!wrap) {
    wrap = el("div", { class: "toast-wrap" });
    document.body.append(wrap);
  }
  const t = el("div", { class: `toast ${kind}` }, [text]);
  wrap.append(t);
  setTimeout(() => {
    t.style.opacity = "0";
    t.style.transition = "opacity .3s";
    setTimeout(() => t.remove(), 320);
  }, 2600);
}

export function modal(title: string, body: HTMLElement, actions: HTMLElement[]): HTMLElement {
  const mask = el("div", { class: "modal-mask" });
  const box = el("div", { class: "modal" }, [
    el("h2", {}, [title]),
    body,
    el("div", { class: "actions" }, actions),
  ]);
  mask.append(box);
  mask.addEventListener("click", (e) => {
    if (e.target === mask) mask.remove();
  });
  document.body.append(mask);
  return mask;
}

/** 无边框窗口标题栏 */
export function buildTitlebar(mode: "teacher" | "screen"): HTMLElement {
  const win = getCurrentWindow();
  const bar = el("header", { class: "titlebar" });

  const logo = el("img", { class: "logo", src: "/logo.png", alt: "" }) as HTMLImageElement;
  const brand = el("div", { class: "brand" }, [
    logo,
    el("span", {}, ["Class Broadcast"]),
    el("span", { class: "studio" }, ["新启年工作室"]),
  ]);

  const chip = el(
    "span",
    { class: `mode-chip ${mode === "teacher" ? "chip-teacher" : "chip-screen"}` },
    [mode === "teacher" ? "教师端 · 发送" : "大屏管理端 · 接收"]
  );

  const controls = el("div", { class: "win-controls" }, [
    el("button", { title: "最小化", onclick: () => void win.minimize() }, []),
    el("button", { title: "最大化/还原", onclick: () => void win.toggleMaximize() }, []),
    el("button", { class: "close", title: "关闭", onclick: () => void win.close() }, []),
  ]);
  // 用内联 SVG 绘制窗口按钮
  const [minBtn, maxBtn, closeBtn] = Array.from(controls.children) as HTMLButtonElement[];
  minBtn.innerHTML = `<svg width="12" height="12" viewBox="0 0 12 12"><rect x="1" y="5.4" width="10" height="1.2" fill="currentColor"/></svg>`;
  maxBtn.innerHTML = `<svg width="12" height="12" viewBox="0 0 12 12"><rect x="1.6" y="1.6" width="8.8" height="8.8" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>`;
  closeBtn.innerHTML = `<svg width="12" height="12" viewBox="0 0 12 12"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>`;

  bar.append(brand, el("div", { class: "spacer" }), chip, controls);
  bar.addEventListener("dblclick", (e) => {
    if ((e.target as HTMLElement).closest(".win-controls")) return;
    void win.toggleMaximize();
  });
  return bar;
}

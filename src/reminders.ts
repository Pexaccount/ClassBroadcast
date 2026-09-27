/**
 * 提醒覆盖层 —— 四种提醒样式
 * 顶部胶囊 / 全屏划入 / CI 接管（ClassIsland 风格） / CW 接管（ClassWidget 风格）
 */

import type { IncomingMessage, ReminderStyle } from "./types";
import { el, svgIcon } from "./ui";

let layer: HTMLElement | null = null;
let seq = 0;

function ensureLayer(): HTMLElement {
  if (!layer || !document.body.contains(layer)) {
    layer = el("div", { class: "reminder-layer" });
    document.body.append(layer);
  }
  return layer;
}

/** 展示一条提醒，返回自动关闭的时长 ms */
export function showReminder(style: ReminderStyle, msg: IncomingMessage, durationMs = 8000): void {
  const root = ensureLayer();
  const id = `rc-${++seq}`;
  let node: HTMLElement;
  let outClass = "out";

  switch (style) {
    case "capsule": {
      node = el("div", { class: "rc-capsule", id }, [
        svgIcon("bell", 20),
        el("span", { class: "who" }, [msg.from]),
        el("span", { class: "what" }, [msg.text]),
      ]);
      break;
    }
    case "fullscreen": {
      node = el("div", { class: "rc-full", id }, [
        el("div", { class: "who" }, [svgIcon("bell", 24), `${msg.from} 发起广播`]),
        el("div", { class: "what" }, [msg.text]),
        el("div", {
          class: "bar",
          style: `width:0%;transition:width ${durationMs}ms linear;`,
        }),
      ]);
      // 进度条
      requestAnimationFrame(() => {
        const bar = node.querySelector<HTMLElement>(".bar");
        if (bar) bar.style.width = "100%";
      });
      break;
    }
    case "class_island": {
      node = el("div", { class: "rc-ci", id }, [
        el("div", { class: "avatar" }, [svgIcon("bell", 24)]),
        el("div", {}, [
          el("div", { class: "who" }, [`${msg.from} · 班级广播`]),
          el("div", { class: "what" }, [msg.text]),
        ]),
      ]);
      break;
    }
    case "class_widget": {
      node = el("div", { class: "rc-cw", id }, [
        el("span", { class: "who" }, [msg.from]),
        el("div", { class: "what" }, [msg.text]),
      ]);
      break;
    }
  }

  root.append(node);

  setTimeout(() => {
    node.classList.add(outClass);
    setTimeout(() => node.remove(), 500);
  }, durationMs);
}

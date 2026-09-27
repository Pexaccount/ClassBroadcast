/**
 * Class Broadcast 云端中继参考实现（可选组件）
 * 用途：校园网之间跨网段推送时，充当消息转发站。
 * 局域网模式完全不依赖本服务；消息体自带 HMAC 签名，中继只转发、不持有密钥。
 *
 * 启动：node relay-server.mjs [端口，默认 8787]
 * 教师端/大屏端「设置」中填入 http://<本机地址>:8787 并勾选启用云端链路。
 */

import http from "node:http";

const PORT = Number(process.argv[2]) || 8787;
/** 消息保留窗口（ms），过期自动清理 */
const TTL_MS = 10 * 60 * 1000;
const MAX_STORE = 500;

/** @type {Array<{pkt:object, ts:number}>} */
const store = [];

const server = http.createServer((req, res) => {
  res.setHeader("Content-Type", "application/json; charset=utf-8");

  if (req.method === "POST" && req.url === "/api/send") {
    let body = "";
    let size = 0;
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size > 128 * 1024) req.destroy();
      else body += chunk;
    });
    req.on("end", () => {
      try {
        const pkt = JSON.parse(body);
        if (pkt?.kind !== "msg" || typeof pkt.text !== "string" || pkt.text.length > 2000) {
          res.writeHead(400).end(JSON.stringify({ error: "bad packet" }));
          return;
        }
        store.push({ pkt, ts: pkt.ts || Date.now() });
        if (store.length > MAX_STORE) store.splice(0, store.length - MAX_STORE);
        res.writeHead(200).end(JSON.stringify({ ok: true }));
      } catch {
        res.writeHead(400).end(JSON.stringify({ error: "bad json" }));
      }
    });
    return;
  }

  if (req.method === "GET" && req.url?.startsWith("/api/poll")) {
    const u = new URL(req.url, "http://localhost");
    const since = Number(u.searchParams.get("since")) || 0;
    const now = Date.now();
    while (store.length && now - store[0].ts > TTL_MS) store.shift();
    const out = store.filter((m) => m.ts > since).map((m) => m.pkt);
    res.writeHead(200).end(JSON.stringify(out));
    return;
  }

  res.writeHead(404).end(JSON.stringify({ error: "not found" }));
});

server.listen(PORT, () => {
  console.log(`[ClassBroadcast-Relay] listening on :${PORT}`);
});

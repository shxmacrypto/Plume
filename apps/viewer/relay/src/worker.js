// Plume Viewer relay (Cloudflare Worker + Durable Object).
//
// A PC running Plume Viewer keeps a WebSocket open on /ws/host?id=<its 9-digit ID>&token=<its secret>.
// A viewer (phone, browser) opens /ws/join?id=<ID>. The room of that ID forwards their messages, nothing else:
// the screen and the keyboard go directly from device to device (WebRTC, encrypted end to end), and the relay
// never learns the password or the one-time code (the viewer proves it knows them to the PC, not to the relay).
//
// The first PC to claim an ID keeps it: the room remembers a hash of its token and refuses any other.

import { DurableObject } from "cloudflare:workers";

const MAX = 64 * 1024; // a signalling message is small (an SDP is a few KB)
const JOINS_PER_MINUTE = 20; // guesses against one ID are throttled here too, the PC also locks after failures

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/ws/host" || url.pathname === "/ws/join") {
      const id = url.searchParams.get("id") || "";
      if (!/^\d{9}$/.test(id)) return new Response("bad id", { status: 400 });
      if (request.headers.get("Upgrade") !== "websocket") return new Response("websocket only", { status: 426 });
      return env.ROOMS.get(env.ROOMS.idFromName(id)).fetch(request);
    }
    if (url.pathname === "/health") return new Response("ok");
    return env.ASSETS.fetch(request);
  },
};

async function sha256hex(text) {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

export class Room extends DurableObject {
  async fetch(request) {
    const url = new URL(request.url);
    const [client, server] = Object.values(new WebSocketPair());
    if (url.pathname === "/ws/host") {
      const token = url.searchParams.get("token") || "";
      if (token.length < 32 || token.length > 128) return new Response("bad token", { status: 400 });
      const hash = await sha256hex(token);
      const known = await this.ctx.storage.get("host");
      if (known && known !== hash) return new Response("this ID belongs to another PC", { status: 403 });
      if (!known) await this.ctx.storage.put("host", hash);
      for (const old of this.ctx.getWebSockets("host")) old.close(4000, "replaced");
      this.ctx.acceptWebSocket(server, ["host"]);
      return new Response(null, { status: 101, webSocket: client });
    }
    // a viewer: throttled, then announced to the PC
    const now = Date.now();
    const recent = ((await this.ctx.storage.get("joins")) || []).filter((t) => now - t < 60_000);
    if (recent.length >= JOINS_PER_MINUTE) return new Response("too many attempts, wait a minute", { status: 429 });
    recent.push(now);
    await this.ctx.storage.put("joins", recent);
    const tag = "c-" + crypto.randomUUID().replaceAll("-", "").slice(0, 16);
    this.ctx.acceptWebSocket(server, ["viewer", tag]);
    const host = this.ctx.getWebSockets("host")[0];
    if (host) {
      host.send(JSON.stringify({ t: "join", from: tag }));
    } else {
      server.send(JSON.stringify({ t: "offline" }));
      server.close(4004, "offline");
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws, message) {
    if (typeof message !== "string" || message.length > MAX) return;
    let msg;
    try {
      msg = JSON.parse(message);
    } catch {
      return;
    }
    const tags = this.ctx.getTags(ws);
    if (tags.includes("host")) {
      // { to: <viewer>, m: <message>, close?: true }
      if (msg.t === "ping") return ws.send('{"t":"pong"}');
      if (typeof msg.to !== "string" || !msg.to.startsWith("c-")) return;
      for (const v of this.ctx.getWebSockets(msg.to)) {
        if (msg.m !== undefined) v.send(JSON.stringify(msg.m));
        if (msg.close) v.close(4001, "closed by the PC");
      }
      return;
    }
    const from = tags.find((t) => t.startsWith("c-"));
    const host = this.ctx.getWebSockets("host")[0];
    if (!host) {
      ws.send(JSON.stringify({ t: "offline" }));
      return;
    }
    host.send(JSON.stringify({ from, m: msg }));
  }

  async webSocketClose(ws, code) {
    const tags = this.ctx.getTags(ws);
    if (tags.includes("host")) {
      for (const v of this.ctx.getWebSockets("viewer")) v.send(JSON.stringify({ t: "offline" }));
    } else {
      const from = tags.find((t) => t.startsWith("c-"));
      for (const h of this.ctx.getWebSockets("host")) h.send(JSON.stringify({ t: "gone", from }));
    }
    try {
      ws.close(code === 1005 ? 1000 : code, "bye");
    } catch {
      // already closed
    }
  }

  async webSocketError(ws) {
    await this.webSocketClose(ws, 1011);
  }
}

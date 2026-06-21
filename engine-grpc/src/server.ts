/**
 * server.ts — gRPC streaming server for vero-core-engine events.
 *
 * Connects to the engine-bridge WebSocket (ZkStateSyncer), fans incoming
 * EngineEvents out to all active gRPC streaming subscribers, and exposes
 * WatchEvents, WatchZkState, and GetHealth RPCs.
 */

import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import * as path from "path";
import { WebSocket } from "ws";

// ── Proto loading (dynamic — no codegen required at runtime) ──────────────

const PROTO_PATH = path.join(__dirname, "..", "proto", "engine.proto");

const pkgDef = protoLoader.loadSync(PROTO_PATH, {
  longs:    String,
  enums:    String,
  defaults: true,
  oneofs:   true,
});

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const proto = (grpc.loadPackageDefinition(pkgDef) as any).vero.engine.v1;

// ── Types mirroring proto messages ────────────────────────────────────────

interface EngineEventMsg {
  id:         string;
  contract_id: string;
  topic:      string[];
  value:      Buffer;
  ledger:     number;
  timestamp:  string;
}

interface ZkStateUpdateMsg {
  event_id:   string;
  contract_id: string;
  ledger:     number;
  timestamp:  string;
  raw:        Buffer;
}

interface WatchRequest {
  contract_id:  string;
  topic_filter: string;
  cursor:       string;
}

// ── Subscriber registry ───────────────────────────────────────────────────

type EventCall   = grpc.ServerWritableStream<WatchRequest, EngineEventMsg>;
type ZkCall      = grpc.ServerWritableStream<WatchRequest, ZkStateUpdateMsg>;

const eventSubs = new Set<EventCall>();
const zkSubs    = new Set<ZkCall>();

function broadcast(event: EngineEventMsg): void {
  for (const call of eventSubs) {
    const req = call.request;
    if (req.contract_id && req.contract_id !== event.contract_id) continue;
    if (req.topic_filter && !event.topic.some(t => t.includes(req.topic_filter))) continue;
    call.write(event);
  }

  const isZk = event.topic.some(t => t.includes("state_commitment"));
  if (isZk) {
    const update: ZkStateUpdateMsg = {
      event_id:    event.id,
      contract_id: event.contract_id,
      ledger:      event.ledger,
      timestamp:   event.timestamp,
      raw:         event.value,
    };
    for (const call of zkSubs) {
      const req = call.request;
      if (req.contract_id && req.contract_id !== event.contract_id) continue;
      call.write(update);
    }
  }
}

// ── WebSocket connection to engine-bridge ─────────────────────────────────

let wsConnected = false;
let wsLiveNodes = 0;
let wsCursor    = "";
let wsUptime    = 0;
const startTime = Date.now();

function connectBridge(wsUrl: string): void {
  const ws = new WebSocket(wsUrl);

  ws.on("open", () => {
    wsConnected = true;
    console.log(`[gRPC] Connected to bridge at ${wsUrl}`);
  });

  ws.on("message", (data: Buffer) => {
    try {
      const msg = JSON.parse(data.toString());

      if (msg.type === "zk_state_update") {
        // ZkStateSnapshot from ZkStateSyncer — synthesise an EngineEvent
        const event: EngineEventMsg = {
          id:          msg.eventId,
          contract_id: msg.contractId,
          topic:       ["state_commitment"],
          value:       Buffer.from(msg.raw ? JSON.stringify(msg.raw) : ""),
          ledger:      msg.ledger,
          timestamp:   msg.timestamp,
        };
        wsCursor = msg.eventId;
        broadcast(event);
      }
    } catch {
      // non-JSON frames (pings, etc.) — ignore
    }
  });

  ws.on("close", () => {
    wsConnected = false;
    console.warn("[gRPC] Bridge WebSocket closed — reconnecting in 5s");
    setTimeout(() => connectBridge(wsUrl), 5_000);
  });

  ws.on("error", (err) => {
    console.error("[gRPC] Bridge WebSocket error:", err.message);
  });

  // Uptime ticker
  setInterval(() => { wsUptime = Math.round((Date.now() - startTime) / 1000); }, 1_000);
}

// ── gRPC service handlers ─────────────────────────────────────────────────

function watchEvents(call: EventCall): void {
  eventSubs.add(call);
  call.on("cancelled", () => eventSubs.delete(call));
  call.on("close",     () => eventSubs.delete(call));
  call.on("error",     () => eventSubs.delete(call));
}

function watchZkState(call: ZkCall): void {
  zkSubs.add(call);
  call.on("cancelled", () => zkSubs.delete(call));
  call.on("close",     () => zkSubs.delete(call));
  call.on("error",     () => zkSubs.delete(call));
}

function getHealth(
  _call: grpc.ServerUnaryCall<Record<string, never>, unknown>,
  cb: grpc.sendUnaryData<unknown>,
): void {
  cb(null, {
    status:         wsConnected ? "ok" : "degraded",
    live_rpc_nodes: wsLiveNodes,
    cursor:         wsCursor,
    uptime_sec:     wsUptime,
  });
}

// ── Server bootstrap ──────────────────────────────────────────────────────

/** Exposed for unit tests — inject synthetic events without a live WebSocket. */
export const _testBroadcast = broadcast;

export function createServer(): grpc.Server {
  const server = new grpc.Server();
  server.addService(proto.EngineService.service, {
    watchEvents,
    watchZkState,
    getHealth,
  });
  return server;
}

if (require.main === module) {
  const port     = process.env.PORT         || "50051";
  const bridgeWs = process.env.BRIDGE_WS_URL || "ws://localhost:8080";

  const server = createServer();
  server.bindAsync(
    `0.0.0.0:${port}`,
    grpc.ServerCredentials.createInsecure(),
    (err, boundPort) => {
      if (err) { console.error("[gRPC] Bind failed:", err); process.exit(1); }
      console.log(`[gRPC] Server listening on port ${boundPort}`);
      connectBridge(bridgeWs);
    },
  );
}

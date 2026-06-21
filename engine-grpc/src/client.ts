/**
 * client.ts — Typed gRPC client for the EngineService streaming API.
 *
 * Wraps the raw gRPC stubs in ergonomic async iterators and a simple
 * getHealth() call.  Handles automatic channel management.
 *
 * Usage:
 *   const client = new EngineClient("localhost:50051");
 *   for await (const event of client.watchEvents({ contract_id: "C..." })) {
 *     console.log(event);
 *   }
 *   const health = await client.getHealth();
 *   client.close();
 */

import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import * as path from "path";

const PROTO_PATH = path.join(__dirname, "..", "proto", "engine.proto");

const pkgDef = protoLoader.loadSync(PROTO_PATH, {
  longs:    String,
  enums:    String,
  defaults: true,
  oneofs:   true,
});

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const proto = (grpc.loadPackageDefinition(pkgDef) as any).vero.engine.v1;

// ── Public interfaces ─────────────────────────────────────────────────────

export interface WatchFilter {
  contract_id?:  string;
  topic_filter?: string;
  cursor?:       string;
}

export interface EngineEvent {
  id:          string;
  contract_id: string;
  topic:       string[];
  value:       Buffer;
  ledger:      number;
  timestamp:   string;
}

export interface ZkStateUpdate {
  event_id:    string;
  contract_id: string;
  ledger:      number;
  timestamp:   string;
  raw:         Buffer;
}

export interface HealthResponse {
  status:          string;
  live_rpc_nodes:  number;
  cursor:          string;
  uptime_sec:      number;
}

// ── Client ────────────────────────────────────────────────────────────────

export class EngineClient {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private readonly stub: any;

  constructor(
    address: string,
    credentials: grpc.ChannelCredentials = grpc.credentials.createInsecure(),
  ) {
    this.stub = new proto.EngineService(address, credentials);
  }

  /** Async iterator over EngineEvents. Ends when the stream closes. */
  async *watchEvents(filter: WatchFilter = {}): AsyncGenerator<EngineEvent> {
    yield* this.readStream<EngineEvent>(this.stub.watchEvents(filter));
  }

  /** Async iterator over ZK state-commitment updates. */
  async *watchZkState(filter: WatchFilter = {}): AsyncGenerator<ZkStateUpdate> {
    yield* this.readStream<ZkStateUpdate>(this.stub.watchZkState(filter));
  }

  /** Unary health check. */
  getHealth(): Promise<HealthResponse> {
    return new Promise((resolve, reject) => {
      this.stub.getHealth({}, (err: grpc.ServiceError | null, res: HealthResponse) => {
        if (err) reject(err); else resolve(res);
      });
    });
  }

  /** Close the underlying gRPC channel. */
  close(): void {
    this.stub.close();
  }

  private async *readStream<T>(
    stream: grpc.ClientReadableStream<T>,
  ): AsyncGenerator<T> {
    const queue: T[]           = [];
    const waiters: Array<(v: IteratorResult<T>) => void> = [];
    let done  = false;
    let error: Error | null    = null;

    stream.on("data",  (chunk: T) => {
      if (waiters.length > 0) {
        waiters.shift()!({ value: chunk, done: false });
      } else {
        queue.push(chunk);
      }
    });

    stream.on("end", () => {
      done = true;
      for (const w of waiters) w({ value: undefined as unknown as T, done: true });
      waiters.length = 0;
    });

    stream.on("error", (err: Error) => {
      done  = true;
      error = err;
      for (const w of waiters) w({ value: undefined as unknown as T, done: true });
      waiters.length = 0;
    });

    while (true) {
      if (queue.length > 0) {
        yield queue.shift()!;
      } else if (done) {
        if (error) throw error;
        return;
      } else {
        const result = await new Promise<IteratorResult<T>>(r => waiters.push(r));
        if (result.done) {
          if (error) throw error;
          return;
        }
        yield result.value;
      }
    }
  }
}

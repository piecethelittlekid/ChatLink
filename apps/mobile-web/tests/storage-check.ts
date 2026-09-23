import { ulid } from "ulid";
import type { ChatMessage } from "@chatlink/protocol";
import {
  acknowledgeOutgoing, importPending, loadLocalState, queueOutgoing, saveIncoming, saveSyncBatch,
} from "../src/storage";

document.body.dataset.result = "MODULE";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

async function run() {
  const phase = new URLSearchParams(location.search).get("phase");
  if (phase === "write") {
    document.body.dataset.result = "WRITE_STARTED";
    const pending: ChatMessage = {
      id: ulid(), sender: "iphone", content: "persist across browser restart",
      status: "pending", createdAt: new Date().toISOString(),
    };
    await queueOutgoing(pending);
    document.body.dataset.result = "QUEUED";
    localStorage.setItem("chatlink-storage-test-id", pending.id);
    const saved = await loadLocalState();
    document.body.dataset.result = "LOADED";
    assert(saved.outbox.some((message) => message.id === pending.id), "pending message was not persisted");
  } else if (phase === "read") {
    const id = localStorage.getItem("chatlink-storage-test-id");
    assert(id, "test ID missing after browser restart");
    const reloaded = await loadLocalState();
    assert(reloaded.outbox.some((message) => message.id === id), "outbox missing after browser restart");
    const acknowledged = await acknowledgeOutgoing(id, 1, new Date().toISOString());
    assert(acknowledged?.status === "stored", "ack did not update stored status");
    const incoming: ChatMessage = {
      id: ulid(), seq: 2, sender: "windows", content: "durable incoming",
      status: "stored", createdAt: new Date().toISOString(),
    };
    await saveIncoming(incoming);
    await saveSyncBatch([incoming], 2);
    const after = await loadLocalState();
    assert(after.outbox.every((message) => message.id !== id), "ack did not clear outbox");
    assert(after.messages.some((message) => message.id === incoming.id), "incoming message missing");
    assert(after.cursor === 2, "sync cursor was not persisted");
    const imported: ChatMessage = {
      id: ulid(), sender: "iphone", content: "from old IP",
      status: "pending", createdAt: new Date().toISOString(),
    };
    await importPending([imported]);
    assert((await loadLocalState()).outbox.some((message) => message.id === imported.id), "import failed");
  } else {
    throw new Error("unknown phase");
  }
  document.body.dataset.result = "PASS";
}

void run().catch((error) => {
  document.body.dataset.result = "FAIL";
  document.body.textContent = String(error);
});

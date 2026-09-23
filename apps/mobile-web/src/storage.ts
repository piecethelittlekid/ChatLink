import type { ChatMessage, Credentials } from "@chatlink/protocol";

const DB_NAME = "chatlink-v1";
const DB_VERSION = 1;
let opening: Promise<IDBDatabase> | undefined;

function database(): Promise<IDBDatabase> {
  if (opening) return opening;
  opening = new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains("settings")) db.createObjectStore("settings");
      if (!db.objectStoreNames.contains("messages")) db.createObjectStore("messages", { keyPath: "id" });
      if (!db.objectStoreNames.contains("outbox")) db.createObjectStore("outbox", { keyPath: "id" });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("Không mở được IndexedDB."));
    request.onblocked = () => reject(new Error("IndexedDB đang bị một cửa sổ khác chặn."));
  }).catch((error) => {
    opening = undefined;
    throw error;
  });
  return opening!;
}

function result<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("IndexedDB request failed."));
  });
}

function finished(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error ?? new Error("IndexedDB transaction failed."));
    transaction.onabort = () => reject(transaction.error ?? new Error("IndexedDB transaction aborted."));
  });
}

export async function loadLocalState(): Promise<{ credentials: Credentials | null; cursor: number; messages: ChatMessage[]; outbox: ChatMessage[] }> {
  const db = await database();
  const tx = db.transaction(["settings", "messages", "outbox"], "readonly");
  const settings = tx.objectStore("settings");
  const credentials = await result(settings.get("credentials")) as Credentials | undefined;
  const cursor = await result(settings.get("lastSeq")) as number | undefined;
  const messages = await result(tx.objectStore("messages").getAll()) as ChatMessage[];
  const outbox = await result(tx.objectStore("outbox").getAll()) as ChatMessage[];
  await finished(tx);
  return { credentials: credentials ?? null, cursor: cursor ?? 0, messages, outbox };
}

export async function saveCredentials(credentials: Credentials): Promise<void> {
  const db = await database();
  const tx = db.transaction("settings", "readwrite");
  tx.objectStore("settings").put(credentials, "credentials");
  await finished(tx);
}

export async function clearCredentials(): Promise<void> {
  const db = await database();
  const tx = db.transaction("settings", "readwrite");
  tx.objectStore("settings").delete("credentials");
  await finished(tx);
}

export async function queueOutgoing(message: ChatMessage): Promise<void> {
  const db = await database();
  const tx = db.transaction(["messages", "outbox"], "readwrite");
  tx.objectStore("messages").put(message);
  tx.objectStore("outbox").put(message);
  await finished(tx);
}

export async function acknowledgeOutgoing(id: string, seq: number, createdAt: string): Promise<ChatMessage | null> {
  const db = await database();
  const tx = db.transaction(["messages", "outbox"], "readwrite");
  const store = tx.objectStore("messages");
  let updated: ChatMessage | null = null;
  const read = store.get(id);
  read.onsuccess = () => {
    const current = read.result as ChatMessage | undefined;
    if (current) {
      updated = { ...current, seq, createdAt, status: "stored" };
      store.put(updated);
    }
    tx.objectStore("outbox").delete(id);
  };
  await finished(tx);
  return updated;
}

export async function saveIncoming(message: ChatMessage): Promise<void> {
  const db = await database();
  const tx = db.transaction("messages", "readwrite");
  tx.objectStore("messages").put(message);
  await finished(tx);
}

export async function saveSyncBatch(messages: ChatMessage[], nextSeq: number): Promise<void> {
  const db = await database();
  const tx = db.transaction(["settings", "messages", "outbox"], "readwrite");
  const stored = tx.objectStore("messages");
  const outbox = tx.objectStore("outbox");
  for (const message of messages) {
    stored.put(message);
    if (message.sender === "iphone") outbox.delete(message.id);
  }
  tx.objectStore("settings").put(nextSeq, "lastSeq");
  await finished(tx);
}

export async function importPending(messages: ChatMessage[]): Promise<void> {
  const db = await database();
  const tx = db.transaction(["messages", "outbox"], "readwrite");
  const stored = tx.objectStore("messages");
  const outbox = tx.objectStore("outbox");
  for (const message of messages) {
    stored.put(message);
    outbox.put(message);
  }
  await finished(tx);
}

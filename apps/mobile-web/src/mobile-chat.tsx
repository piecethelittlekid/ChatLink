import { useCallback, useEffect, useRef, useState, type ChangeEvent, type FormEvent } from "react";
import { createEnvelope, messageIdSchema, type ChatMessage, type ConnectionStatus, type Credentials } from "@chatlink/protocol";
import { ulid } from "ulid";
import {
  acknowledgeOutgoing, clearCredentials, importPending, loadLocalState, queueOutgoing,
  saveCredentials, saveIncoming, saveSyncBatch,
} from "./storage";

type ServerEnvelope = {
  type: string;
  requestId?: string;
  timestamp?: string;
  payload?: Record<string, unknown>;
};

const BACKOFF_MS = [1000, 2000, 4000, 8000, 10000];
const MAX_BYTES = 8192;

function socketUrl(serverUrl: string) {
  const url = new URL(serverUrl);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.pathname = "/ws";
  url.search = "";
  return url.toString();
}

function mergeMessages(current: ChatMessage[], incoming: ChatMessage[]): ChatMessage[] {
  const byId = new Map(current.map((message) => [message.id, message]));
  for (const message of incoming) {
    byId.set(message.id, { ...byId.get(message.id), ...message });
  }
  return [...byId.values()].sort((a, b) => {
    if (a.seq !== undefined && b.seq !== undefined) return a.seq - b.seq;
    return a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id);
  });
}

function parseStoredMessage(raw: unknown): ChatMessage {
  if (!raw || typeof raw !== "object") throw new Error("Tin đồng bộ không hợp lệ.");
  const value = raw as Record<string, unknown>;
  if (!messageIdSchema.safeParse(value.id).success
    || typeof value.seq !== "number" || !Number.isSafeInteger(value.seq) || value.seq < 1
    || (value.sender !== "windows" && value.sender !== "iphone")
    || typeof value.content !== "string" || new TextEncoder().encode(value.content).length > MAX_BYTES
    || typeof value.createdAt !== "string"
    || (value.status !== "stored" && value.status !== "delivered")) {
    throw new Error("Tin đồng bộ không hợp lệ.");
  }
  return {
    id: value.id as string, seq: value.seq, sender: value.sender,
    content: value.content, createdAt: value.createdAt, status: value.status,
  };
}

export function MobileChat() {
  const [serverUrl, setServerUrl] = useState(window.location.origin);
  const [credentials, setCredentials] = useState<Credentials | null>(null);
  const [pairingCode, setPairingCode] = useState("");
  const [codeInput, setCodeInput] = useState("");
  const [storageReady, setStorageReady] = useState(false);
  const [connection, setConnection] = useState<ConnectionStatus>("disconnected");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [pendingCount, setPendingCount] = useState(0);
  const [sending, setSending] = useState(false);
  const socketRef = useRef<WebSocket | null>(null);
  const credentialsRef = useRef<Credentials | null>(null);
  const pairingCodeRef = useRef("");
  const outboxRef = useRef(new Map<string, ChatMessage>());
  const cursorRef = useRef(0);
  const syncCompleteRef = useRef(false);
  const shouldReconnectRef = useRef(false);
  const attemptsRef = useRef(0);
  const retryTimerRef = useRef<number | undefined>(undefined);
  const bottomRef = useRef<HTMLDivElement>(null);

  const addOrUpdate = useCallback((incoming: ChatMessage[]) => {
    setMessages((current) => mergeMessages(current, incoming));
  }, []);

  const connect = useCallback(() => {
    if (socketRef.current && (socketRef.current.readyState === WebSocket.OPEN || socketRef.current.readyState === WebSocket.CONNECTING)) return;
    if (!credentialsRef.current && !pairingCodeRef.current) return;
    setConnection("connecting");
    let ws: WebSocket;
    try {
      ws = new WebSocket(socketUrl(serverUrl));
    } catch {
      setConnection("disconnected");
      setError("Địa chỉ máy tính không hợp lệ.");
      return;
    }
    socketRef.current = ws;
    syncCompleteRef.current = false;
    let processing = Promise.resolve();

    function flushOutbox() {
      if (ws.readyState !== WebSocket.OPEN) return;
      for (const pending of outboxRef.current.values()) {
        ws.send(JSON.stringify(createEnvelope("chat_message", { content: pending.content }, pending.id)));
      }
    }

    ws.onopen = () => {
      const saved = credentialsRef.current;
      if (saved) ws.send(JSON.stringify(createEnvelope("auth", saved)));
      else ws.send(JSON.stringify(createEnvelope("pairing_request", { code: pairingCodeRef.current })));
    };
    ws.onmessage = (event) => {
      processing = processing.then(async () => {
        if (socketRef.current !== ws) return;
        let envelope: ServerEnvelope;
        try { envelope = JSON.parse(String(event.data)) as ServerEnvelope; } catch { throw new Error("Dữ liệu server không hợp lệ."); }
        const payload = envelope.payload ?? {};
        if (envelope.type === "pairing_ok") {
          if (typeof payload.deviceId !== "string" || typeof payload.token !== "string") throw new Error("Thông tin ghép đôi không hợp lệ.");
          const saved = { deviceId: payload.deviceId, token: payload.token };
          await saveCredentials(saved);
          credentialsRef.current = saved;
          pairingCodeRef.current = "";
          setCredentials(saved);
          setPairingCode("");
          ws.send(JSON.stringify(createEnvelope("auth", saved)));
        } else if (envelope.type === "auth_ok") {
          attemptsRef.current = 0;
          setError("");
          ws.send(JSON.stringify(createEnvelope("sync_request", { afterSeq: cursorRef.current })));
        } else if (envelope.type === "sync_batch") {
          if (!Array.isArray(payload.messages) || typeof payload.nextSeq !== "number" || typeof payload.hasMore !== "boolean") {
            throw new Error("Phản hồi đồng bộ không hợp lệ.");
          }
          const batch = payload.messages.map(parseStoredMessage);
          if (payload.nextSeq < cursorRef.current || !Number.isSafeInteger(payload.nextSeq)) throw new Error("Con trỏ đồng bộ không hợp lệ.");
          await saveSyncBatch(batch, payload.nextSeq);
          cursorRef.current = payload.nextSeq;
          for (const message of batch) {
            if (message.sender === "iphone") outboxRef.current.delete(message.id);
            if (message.sender === "windows" && message.status !== "delivered" && ws.readyState === WebSocket.OPEN) {
              ws.send(JSON.stringify(createEnvelope("delivered", { messageId: message.id })));
            }
          }
          setPendingCount(outboxRef.current.size);
          addOrUpdate(batch);
          if (ws.readyState === WebSocket.OPEN) {
            if (payload.hasMore) ws.send(JSON.stringify(createEnvelope("sync_request", { afterSeq: cursorRef.current })));
            else {
              syncCompleteRef.current = true;
              setConnection("connected");
              flushOutbox();
            }
          }
        } else if (envelope.type === "message_ack") {
          const id = payload.messageId;
          if (typeof id !== "string" || !messageIdSchema.safeParse(id).success
            || typeof payload.seq !== "number" || typeof payload.createdAt !== "string") return;
          const stored = await acknowledgeOutgoing(id, payload.seq, payload.createdAt);
          outboxRef.current.delete(id);
          setPendingCount(outboxRef.current.size);
          if (stored) addOrUpdate([stored]);
        } else if (envelope.type === "chat_message") {
          const incoming = parseStoredMessage({
            id: envelope.requestId, seq: payload.seq, sender: payload.sender,
            content: payload.content, status: "stored", createdAt: envelope.timestamp,
          });
          await saveIncoming(incoming);
          addOrUpdate([incoming]);
          if (incoming.sender === "windows" && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify(createEnvelope("delivered", { messageId: incoming.id })));
          }
        } else if (envelope.type === "ping") {
          if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(createEnvelope("pong", {})));
        } else if (envelope.type === "error") {
          const code = String(payload.code ?? "");
          if (["AUTH_FAILED", "AUTH_REQUIRED", "PAIRING_CODE_INVALID", "ALREADY_PAIRED", "RATE_LIMITED"].includes(code)) {
            shouldReconnectRef.current = false;
            if (code === "AUTH_FAILED") {
              await clearCredentials();
              credentialsRef.current = null;
              setCredentials(null);
            }
            if (!credentialsRef.current) {
              pairingCodeRef.current = "";
              setPairingCode("");
            }
            setError(code === "PAIRING_CODE_INVALID" ? "Mã ghép đôi sai hoặc đã hết hạn."
              : code === "ALREADY_PAIRED" ? "Máy tính đã ghép đôi với một iPhone khác."
                : code === "RATE_LIMITED" ? "Thử quá nhiều lần. Hãy đợi một phút."
                  : "Không xác thực được thiết bị. Hãy ghép đôi lại trên Windows.");
          } else if (code === "MESSAGE_RATE_LIMITED") {
            setError("Gửi quá nhanh; tin pending sẽ được thử lại sau 10 giây.");
            window.setTimeout(flushOutbox, 10000);
          } else {
            setError(code === "DATABASE_ERROR" ? "Máy tính không lưu được tin nhắn." : "Server từ chối tin nhắn. Kiểm tra nội dung và thử lại.");
          }
        }
      }).catch((reason) => {
        setError(String(reason instanceof Error ? reason.message : reason));
        ws.close();
      });
    };
    ws.onerror = () => { if (socketRef.current === ws) setError("Không kết nối được. Kiểm tra Wi-Fi và chứng chỉ ChatLink."); };
    ws.onclose = () => {
      if (socketRef.current !== ws) return;
      socketRef.current = null;
      syncCompleteRef.current = false;
      setConnection("disconnected");
      if (shouldReconnectRef.current && (credentialsRef.current || pairingCodeRef.current)) {
        const delay = BACKOFF_MS[Math.min(attemptsRef.current, BACKOFF_MS.length - 1)];
        attemptsRef.current += 1;
        retryTimerRef.current = window.setTimeout(connect, delay);
      }
    };
  }, [serverUrl, addOrUpdate]);

  useEffect(() => {
    let live = true;
    void loadLocalState().then((saved) => {
      if (!live) return;
      credentialsRef.current = saved.credentials;
      cursorRef.current = saved.cursor;
      outboxRef.current = new Map(saved.outbox.map((message) => [message.id, message]));
      setCredentials(saved.credentials);
      setMessages(mergeMessages(saved.messages, saved.outbox));
      setPendingCount(saved.outbox.length);
      setStorageReady(true);
    }).catch(() => setError("Không mở được IndexedDB. Kiểm tra dung lượng và quyền lưu trữ của Safari."));
    return () => { live = false; };
  }, []);

  useEffect(() => {
    if (!storageReady || (!credentials && !pairingCode)) return;
    shouldReconnectRef.current = true;
    connect();
    const heartbeat = window.setInterval(() => {
      if (socketRef.current?.readyState === WebSocket.OPEN && credentialsRef.current) {
        socketRef.current.send(JSON.stringify(createEnvelope("ping", {})));
      }
    }, 15000);
    const resume = () => { if (document.visibilityState === "visible") connect(); };
    document.addEventListener("visibilitychange", resume);
    return () => { window.clearInterval(heartbeat); document.removeEventListener("visibilitychange", resume); };
  }, [storageReady, credentials, pairingCode, connect]);

  useEffect(() => () => {
    shouldReconnectRef.current = false;
    window.clearTimeout(retryTimerRef.current);
    socketRef.current?.close();
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages.at(-1)?.id]);

  function submitCode(event: FormEvent) {
    event.preventDefault();
    const code = codeInput.trim();
    if (!/^\d{6}$/.test(code)) { setError("Nhập mã ghép đôi gồm 6 chữ số trên Windows."); return; }
    pairingCodeRef.current = code;
    setPairingCode(code);
    setError("");
  }

  async function submitMessage(event: FormEvent) {
    event.preventDefault();
    const content = draft.trim();
    if (!content || !storageReady || sending) return;
    if (new TextEncoder().encode(content).length > MAX_BYTES) { setError("Tin nhắn vượt giới hạn 8 KB."); return; }
    setSending(true);
    const message: ChatMessage = { id: ulid(), sender: "iphone", content, status: "pending", createdAt: new Date().toISOString() };
    try {
      await queueOutgoing(message);
      outboxRef.current.set(message.id, message);
      setPendingCount(outboxRef.current.size);
      addOrUpdate([message]);
      setDraft("");
      setError("");
      if (socketRef.current?.readyState === WebSocket.OPEN && syncCompleteRef.current) {
        socketRef.current.send(JSON.stringify(createEnvelope("chat_message", { content }, message.id)));
      }
    } catch {
      setError("Không lưu được tin pending trên iPhone. Kiểm tra dung lượng lưu trữ.");
    } finally {
      setSending(false);
    }
  }

  async function forgetCredentials() {
    shouldReconnectRef.current = false;
    socketRef.current?.close();
    credentialsRef.current = null;
    pairingCodeRef.current = "";
    await clearCredentials();
    setCredentials(null);
    setPairingCode("");
    setOptionsOpen(false);
    setConnection("disconnected");
  }

  function exportOutbox() {
    const blob = new Blob([JSON.stringify({ version: 1, messages: [...outboxRef.current.values()] }, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "chatlink-outbox.json";
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 60000);
  }

  async function importOutbox(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    try {
      if (file.size > 2_000_000) throw new Error("Tệp quá lớn.");
      const parsed = JSON.parse(await file.text()) as { version?: unknown; messages?: unknown };
      if (parsed.version !== 1 || !Array.isArray(parsed.messages) || parsed.messages.length > 1000) throw new Error("Tệp outbox không hợp lệ.");
      const existing = new Set(messages.map((message) => message.id));
      const pending: ChatMessage[] = [];
      for (const raw of parsed.messages) {
        const value = raw as Partial<ChatMessage>;
        if (!value || !messageIdSchema.safeParse(value.id).success || value.sender !== "iphone"
          || typeof value.content !== "string" || !value.content.trim()
          || new TextEncoder().encode(value.content).length > MAX_BYTES
          || typeof value.createdAt !== "string" || Number.isNaN(Date.parse(value.createdAt))) {
          throw new Error("Tệp chứa tin nhắn không hợp lệ.");
        }
        if (!existing.has(value.id!)) {
          existing.add(value.id!);
          pending.push({ id: value.id!, sender: "iphone", content: value.content, createdAt: value.createdAt, status: "pending" });
        }
      }
      await importPending(pending);
      for (const message of pending) outboxRef.current.set(message.id, message);
      setPendingCount(outboxRef.current.size);
      addOrUpdate(pending);
      if (socketRef.current?.readyState === WebSocket.OPEN && syncCompleteRef.current) {
        for (const message of pending) {
          socketRef.current.send(JSON.stringify(createEnvelope("chat_message", { content: message.content }, message.id)));
        }
      }
      setError(`Đã nhập ${pending.length} tin pending.`);
    } catch (reason) {
      setError(String(reason instanceof Error ? reason.message : reason));
    }
  }

  const transfer = (
    <div className="transfer-actions">
      <button type="button" onClick={exportOutbox} disabled={pendingCount === 0}>Xuất outbox ({pendingCount})</button>
      <label>Nhập outbox<input type="file" accept="application/json,.json" onChange={(event) => void importOutbox(event)} /></label>
    </div>
  );

  if (!storageReady || !credentials) {
    return (
      <main className="mobile-shell setup-shell">
        <div className="setup-brand"><div className="brand-mark">C</div><span>ChatLink</span></div>
        <section className="setup-card">
          <div className="setup-icon">⌁</div>
          <p className="eyebrow">PRIVATE LAN CHAT</p>
          <h1>Ghép đôi với máy tính</h1>
          <p className="setup-copy">Mở ChatLink trên Windows rồi nhập mã ghép đôi đang hiển thị. Chỉ cần làm một lần.</p>
          <form onSubmit={submitCode} className="setup-form">
            {["localhost", "127.0.0.1"].includes(window.location.hostname) && (
              <label className="field-label">Địa chỉ server thử nghiệm
                <input value={serverUrl} onChange={(event) => setServerUrl(event.target.value)} placeholder="https://192.168.1.15:42100" autoCapitalize="none" />
              </label>
            )}
            <label className="field-label">Mã ghép đôi
              <input className="code-input" inputMode="numeric" autoComplete="one-time-code" maxLength={6} value={codeInput} onChange={(event) => setCodeInput(event.target.value.replace(/\D/g, ""))} placeholder="000 000" />
            </label>
            {error && <p className="form-error">{error}</p>}
            <button className="primary-button" type="submit" disabled={!storageReady || !!pairingCode}>{pairingCode ? "Đang ghép đôi…" : "Ghép đôi iPhone"} <span>→</span></button>
            {pairingCode && <button className="cancel-pairing" type="button" onClick={() => {
              shouldReconnectRef.current = false;
              pairingCodeRef.current = "";
              setPairingCode("");
              socketRef.current?.close();
            }}>Nhập lại mã ghép đôi</button>}
          </form>
          {transfer}
          <p className="setup-footnote">Cùng Wi-Fi với Windows. Mã ghép đôi hết hạn sau 10 phút.</p>
        </section>
        <p className="version-note">ChatLink · LAN chat</p>
      </main>
    );
  }

  return (
    <main className="mobile-shell chat-shell">
      <header className="mobile-header">
        <div className="header-brand"><div className="brand-mark small">C</div><div><strong>ChatLink</strong><span>iPhone · Private chat</span></div></div>
        <div className={`connection-pill ${connection}`}><i />{connection === "connected" ? "Đã kết nối" : connection === "connecting" ? "Đang nối" : "Ngoại tuyến"}</div>
        <button className="more-button" aria-label="Tùy chọn kết nối" onClick={() => setOptionsOpen(!optionsOpen)}>···</button>
      </header>
      {optionsOpen && <section className="mobile-options">
        <p>Khi IP máy tính đổi, xuất tin pending từ biểu tượng cũ rồi nhập ở địa chỉ mới.</p>
        {transfer}
        <button type="button" onClick={() => void forgetCredentials()}>Quên ghép đôi trên iPhone</button>
      </section>}
      {error && <div className="connection-banner">{error}</div>}
      <section className="messages-area" aria-live="polite">
        {messages.length === 0 ? (
          <div className="empty-chat"><div className="empty-bubble">✦</div><h1>Bắt đầu cuộc trò chuyện</h1><p>Tin nhắn được lưu trên Windows và đồng bộ lại khi kết nối.</p></div>
        ) : messages.map((message) => (
          <article key={message.id} className={`message-row ${message.sender === "iphone" ? "outgoing" : "incoming"}`}>
            <div className="message-bubble">
              <p>{message.content}</p>
              <div className="message-meta"><time>{new Intl.DateTimeFormat("vi-VN", { hour: "2-digit", minute: "2-digit" }).format(new Date(message.createdAt))}</time>{message.sender === "iphone" && <span>{message.status === "pending" ? "◷" : "✓"}</span>}</div>
            </div>
          </article>
        ))}
        <div ref={bottomRef} />
      </section>
      <form className="composer" onSubmit={(event) => void submitMessage(event)}>
        <textarea value={draft} onChange={(event) => setDraft(event.target.value)} rows={1} maxLength={8192} placeholder="Viết tin nhắn…" aria-label="Tin nhắn" onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }} />
        <button className="send-button" type="submit" aria-label="Gửi tin nhắn" disabled={!draft.trim() || sending}><span>↑</span></button>
      </form>
      <footer className="mobile-footer"><span className="lock-icon">⌑</span> {pendingCount > 0 ? `${pendingCount} tin đang chờ gửi · ` : ""}Tin nhắn lưu trên máy tính</footer>
    </main>
  );
}

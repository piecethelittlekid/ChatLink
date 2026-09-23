import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { createEnvelope, type ChatMessage, type ConnectionStatus, messageIdSchema } from "@chatlink/protocol";
import { ulid } from "ulid";

type ServerEnvelope = {
  type: string;
  requestId?: string;
  timestamp?: string;
  payload?: Record<string, unknown>;
};

const BACKOFF_MS = [1000, 2000, 4000, 8000, 10000];

function now() {
  return new Date().toISOString();
}

function socketUrl(serverUrl: string) {
  const url = new URL(serverUrl);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.pathname = "/ws";
  url.search = "";
  return url.toString();
}

export function MobileChat() {
  const [serverUrl, setServerUrl] = useState(window.location.origin);
  const [code, setCode] = useState("");
  const [codeInput, setCodeInput] = useState("");
  const [connection, setConnection] = useState<ConnectionStatus>("disconnected");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");
  const socketRef = useRef<WebSocket | null>(null);
  const outboxRef = useRef(new Map<string, ChatMessage>());
  const shouldReconnectRef = useRef(false);
  const attemptsRef = useRef(0);
  const retryTimerRef = useRef<number | undefined>(undefined);
  const bottomRef = useRef<HTMLDivElement>(null);

  const addOrUpdateMessage = useCallback((message: ChatMessage) => {
    setMessages((current) => {
      const found = current.some((item) => item.id === message.id);
      return found
        ? current.map((item) => item.id === message.id ? { ...item, ...message } : item)
        : [...current, message].sort((a, b) => a.createdAt.localeCompare(b.createdAt));
    });
  }, []);

  const connect = useCallback((activeCode: string, activeServerUrl: string) => {
    if (!activeCode || socketRef.current?.readyState === WebSocket.OPEN || socketRef.current?.readyState === WebSocket.CONNECTING) return;
    setConnection("connecting");
    setError("");
    let ws: WebSocket;
    try {
      ws = new WebSocket(socketUrl(activeServerUrl));
    } catch {
      setConnection("disconnected");
      setError("Địa chỉ máy tính không hợp lệ.");
      return;
    }
    socketRef.current = ws;

    ws.onopen = () => {
      ws.send(JSON.stringify(createEnvelope("auth", { code: activeCode })));
    };
    ws.onmessage = (event) => {
      let message: ServerEnvelope;
      try { message = JSON.parse(String(event.data)) as ServerEnvelope; } catch { return; }
      const payload = message.payload ?? {};
      if (message.type === "auth_ok") {
        attemptsRef.current = 0;
        setConnection("connected");
        setError("");
        for (const pending of outboxRef.current.values()) {
          ws.send(JSON.stringify(createEnvelope("chat_message", { content: pending.content }, pending.id)));
        }
      } else if (message.type === "error") {
        const errorCode = String(payload.code ?? "");
        if (errorCode === "AUTH_FAILED" || errorCode === "RATE_LIMITED" || errorCode === "AUTH_REQUIRED") {
          shouldReconnectRef.current = false;
          setError(errorCode === "AUTH_FAILED" ? "Mã truy cập không đúng. Hãy kiểm tra lại trên ChatLink Windows." : "Quá nhiều lần thử. Hãy đợi một phút rồi thử lại.");
        } else if (errorCode === "INVALID_MESSAGE") {
          setError("Tin nhắn không hợp lệ hoặc vượt giới hạn 8 KB.");
        } else if (errorCode === "MESSAGE_RATE_LIMITED") {
          setError("Bạn đang gửi quá nhanh. Hãy đợi một chút rồi thử lại.");
        }
      } else if (message.type === "message_ack") {
        const messageId = String(payload.messageId ?? "");
        const pending = outboxRef.current.get(messageId);
        if (pending) {
          outboxRef.current.delete(messageId);
          addOrUpdateMessage({ ...pending, status: "stored" });
        }
      } else if (message.type === "chat_message") {
        const id = String(message.requestId ?? payload.messageId ?? "");
        if (!messageIdSchema.safeParse(id).success) return;
        const incoming: ChatMessage = {
          id,
          sender: payload.sender === "iphone" ? "iphone" : "windows",
          content: String(payload.content ?? ""),
          status: "delivered",
          createdAt: message.timestamp ?? now(),
        };
        addOrUpdateMessage(incoming);
        ws.send(JSON.stringify(createEnvelope("delivered", { messageId: id })));
      } else if (message.type === "ping") {
        ws.send(JSON.stringify(createEnvelope("pong", {})));
      }
    };
    ws.onerror = () => setError("Không kết nối được. Hãy kiểm tra Wi-Fi và tin cậy chứng chỉ ChatLink trên iPhone.");
    ws.onclose = () => {
      if (socketRef.current === ws) socketRef.current = null;
      setConnection("disconnected");
      if (shouldReconnectRef.current) {
        const delay = BACKOFF_MS[Math.min(attemptsRef.current, BACKOFF_MS.length - 1)];
        attemptsRef.current += 1;
        retryTimerRef.current = window.setTimeout(() => connect(activeCode, activeServerUrl), delay);
      }
    };
  }, [addOrUpdateMessage]);

  useEffect(() => () => {
    shouldReconnectRef.current = false;
    window.clearTimeout(retryTimerRef.current);
    socketRef.current?.close();
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages]);

  useEffect(() => {
    if (!code) return;
    shouldReconnectRef.current = true;
    connect(code, serverUrl);
    const heartbeat = window.setInterval(() => {
      if (socketRef.current?.readyState === WebSocket.OPEN) {
        socketRef.current.send(JSON.stringify(createEnvelope("ping", {})));
      }
    }, 15000);
    return () => window.clearInterval(heartbeat);
  }, [code, serverUrl, connect]);

  function submitCode(event: FormEvent) {
    event.preventDefault();
    if (!/^\d{6}$/.test(codeInput.trim())) {
      setError("Nhập mã gồm 6 chữ số đang hiển thị trên ChatLink Windows.");
      return;
    }
    shouldReconnectRef.current = true;
    setCode(codeInput.trim());
  }

  function submitMessage(event: FormEvent) {
    event.preventDefault();
    const content = draft.trim();
    if (!content) return;
    if (new TextEncoder().encode(content).length > 8192) {
      setError("Tin nhắn vượt giới hạn 8 KB.");
      return;
    }
    const message: ChatMessage = { id: ulid(), sender: "iphone", content, status: "pending", createdAt: now() };
    outboxRef.current.set(message.id, message);
    addOrUpdateMessage(message);
    setDraft("");
    if (socketRef.current?.readyState === WebSocket.OPEN && connection === "connected") {
      socketRef.current.send(JSON.stringify(createEnvelope("chat_message", { content }, message.id)));
    }
  }

  function changeSession() {
    shouldReconnectRef.current = false;
    socketRef.current?.close();
    socketRef.current = null;
    setCode("");
    setConnection("disconnected");
    setMessages([]);
    outboxRef.current.clear();
    setError("");
  }

  if (!code) {
    return (
      <main className="mobile-shell setup-shell">
        <div className="setup-brand"><div className="brand-mark">C</div><span>ChatLink</span></div>
        <section className="setup-card">
          <div className="setup-icon">⌁</div>
          <p className="eyebrow">PRIVATE LAN CHAT</p>
          <h1>Kết nối với máy tính</h1>
          <p className="setup-copy">Mở ChatLink trên Windows, rồi nhập mã truy cập đang hiển thị ở đó.</p>
          <form onSubmit={submitCode} className="setup-form">
            {window.location.hostname === "localhost" || window.location.hostname === "127.0.0.1" ? (
              <label className="field-label">Địa chỉ server để thử trên trình duyệt
                <input value={serverUrl} onChange={(event) => setServerUrl(event.target.value)} placeholder="https://192.168.1.15:42100" autoCapitalize="none" />
              </label>
            ) : null}
            <label className="field-label">Mã truy cập
              <input className="code-input" inputMode="numeric" autoComplete="one-time-code" maxLength={6} value={codeInput} onChange={(event) => setCodeInput(event.target.value.replace(/\D/g, ""))} placeholder="000 000" />
            </label>
            {error && <p className="form-error">{error}</p>}
            <button className="primary-button" type="submit">Kết nối với PC <span>→</span></button>
          </form>
          <p className="setup-footnote">Chỉ hoạt động khi iPhone và PC cùng mạng Wi‑Fi.</p>
        </section>
        <p className="version-note">ChatLink · Private by design</p>
      </main>
    );
  }

  return (
    <main className="mobile-shell chat-shell">
      <header className="mobile-header">
        <div className="header-brand"><div className="brand-mark small">C</div><div><strong>ChatLink</strong><span>iPhone · Private chat</span></div></div>
        <div className={`connection-pill ${connection}`}><i />{connection === "connected" ? "Đã kết nối" : connection === "connecting" ? "Đang nối" : "Ngoại tuyến"}</div>
        <button className="more-button" aria-label="Tùy chọn kết nối" onClick={changeSession}>···</button>
      </header>
      {error && <div className="connection-banner">{error}</div>}
      <section className="messages-area" aria-live="polite">
        {messages.length === 0 ? (
          <div className="empty-chat"><div className="empty-bubble">✦</div><h1>Bắt đầu cuộc trò chuyện</h1><p>Tin nhắn được gửi trực tiếp giữa hai thiết bị trong mạng riêng của bạn.</p></div>
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
      <form className="composer" onSubmit={submitMessage}>
        <textarea value={draft} onChange={(event) => setDraft(event.target.value)} rows={1} maxLength={8192} placeholder="Viết tin nhắn…" aria-label="Tin nhắn" onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }} />
        <button className="send-button" type="submit" aria-label="Gửi tin nhắn" disabled={!draft.trim()}><span>↑</span></button>
      </form>
      <footer className="mobile-footer"><span className="lock-icon">⌑</span> Tin nhắn được lưu trên máy tính của bạn</footer>
    </main>
  );
}

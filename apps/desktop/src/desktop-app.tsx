import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";

type Snapshot = {
  status: string;
  interfaces: string[];
  selectedIp: string | null;
  httpsUrl: string | null;
  setupUrl: string | null;
  certificateSetupRequired: boolean;
  caFingerprint: string;
  pairingCode: string | null;
  pairingExpiresAt: string | null;
  pairedDevice: string | null;
  iphoneStatus: string;
  error: string | null;
};

type Message = {
  seq: number;
  id: string;
  sender: "windows" | "iphone";
  content: string;
  status: string;
  createdAt: string;
};

type InterfaceOption = { ip: string; label: string };

const EMPTY_SNAPSHOT: Snapshot = {
  status: "starting", interfaces: [], selectedIp: null, httpsUrl: null, setupUrl: null,
  certificateSetupRequired: true, caFingerprint: "", pairingCode: null, pairingExpiresAt: null, pairedDevice: null, iphoneStatus: "waiting", error: null,
};

function mergeMessages(current: Message[], incoming: Message[]): Message[] {
  const byId = new Map(current.map((message) => [message.id, message]));
  for (const message of incoming) byId.set(message.id, message);
  return [...byId.values()].sort((a, b) => a.seq - b.seq);
}

function formatTime(value: string) {
  return new Intl.DateTimeFormat("vi-VN", { hour: "2-digit", minute: "2-digit" }).format(new Date(value));
}

export function DesktopApp() {
  const [snapshot, setSnapshot] = useState(EMPTY_SNAPSHOT);
  const [interfaces, setInterfaces] = useState<InterfaceOption[]>([]);
  const [messages, setMessages] = useState<Message[]>([]);
  const [hasOlder, setHasOlder] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");
  const [view, setView] = useState<"chat" | "settings">("chat");
  const [copied, setCopied] = useState(false);
  const [certificateInstalled, setCertificateInstalled] = useState(false);
  const [switching, setSwitching] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const historyInitialized = useRef(false);

  async function refresh() {
    try {
      const [nextStatus, nextMessages, nextInterfaces] = await Promise.all([
        invoke<Snapshot>("get_server_status"),
        invoke<Message[]>("get_recent_messages", { beforeSeq: null }),
        invoke<InterfaceOption[]>("list_interfaces"),
      ]);
      setSnapshot(nextStatus);
      setCertificateInstalled(!nextStatus.certificateSetupRequired);
      setMessages((current) => mergeMessages(current, nextMessages));
      if (!historyInitialized.current) {
        setHasOlder(nextMessages.length === 100);
        historyInitialized.current = true;
      }
      setInterfaces(nextInterfaces);
    } catch (reason) {
      setError(String(reason));
    }
  }

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 900);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages.at(-1)?.id]);

  const connected = snapshot.iphoneStatus === "connected";
  const codeGroups = useMemo(() => snapshot.pairingCode?.split("") ?? [], [snapshot.pairingCode]);
  const qrUrl = certificateInstalled ? snapshot.httpsUrl : snapshot.setupUrl;

  async function submitMessage(event: FormEvent) {
    event.preventDefault();
    const content = draft.trim();
    if (!content || snapshot.status !== "online") return;
    if (new TextEncoder().encode(content).length > 8192) {
      setError("Tin nhắn vượt giới hạn 8 KB.");
      return;
    }
    setError("");
    setDraft("");
    try {
      await invoke<Message>("send_message", { content });
    } catch (reason) {
      setDraft(content);
      const detail = String(reason);
      setError(detail.includes("message rate exceeded") ? "Bạn đang gửi quá nhanh. Hãy đợi một chút rồi thử lại." : detail);
    }
  }

  async function changeInterface(ip: string) {
    if (!ip || ip === snapshot.selectedIp) return;
    setSwitching(true);
    setError("");
    try {
      const nextStatus = await invoke<Snapshot>("select_network_interface", { ip });
      setSnapshot(nextStatus);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSwitching(false);
    }
  }

  async function copySetupUrl() {
    const url = certificateInstalled ? snapshot.httpsUrl : snapshot.setupUrl;
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setError("Không sao chép được liên kết. Bạn có thể mở URL bằng điện thoại trong cùng Wi-Fi.");
    }
  }

  async function confirmCertificateSetup() {
    setError("");
    try {
      const nextStatus = await invoke<Snapshot>("confirm_certificate_setup");
      setSnapshot(nextStatus);
      setCertificateInstalled(true);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function loadOlder() {
    if (!messages.length) return;
    try {
      const older = await invoke<Message[]>("get_recent_messages", { beforeSeq: messages[0].seq });
      setMessages((current) => mergeMessages(current, older));
      setHasOlder(older.length === 100);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function unpairIphone() {
    if (!window.confirm("Hủy ghép đôi iPhone? Lịch sử trên Windows vẫn được giữ lại.")) return;
    try {
      const nextStatus = await invoke<Snapshot>("unpair_iphone");
      setSnapshot(nextStatus);
      setError("");
    } catch (reason) {
      setError(String(reason));
    }
  }

  return (
    <div className="desktop-app">
      <aside className="sidebar">
        <div className="desktop-brand"><div className="brand-mark">C</div><div><strong>ChatLink</strong><span>PERSONAL MESSENGER</span></div></div>
        <div className="workspace-label">WORKSPACE</div>
        <button className={`nav-item ${view === "chat" ? "active" : ""}`} onClick={() => setView("chat")}><span className="nav-icon">◉</span> Trò chuyện <span className="nav-count">1</span></button>
        <button className={`nav-item ${view === "settings" ? "active" : ""}`} onClick={() => setView("settings")}><span className="nav-icon">⚙</span> Cài đặt</button>
        <div className="sidebar-spacer" />
        <div className="host-card">
          <div className="host-avatar">W</div>
          <div className="host-name"><strong>Máy tính này</strong><span>Windows host</span></div>
          <span className={`tiny-status ${snapshot.status === "online" ? "online" : "offline"}`} />
        </div>
        <div className="sidebar-version">ChatLink V1 · LAN</div>
      </aside>

      <main className="desktop-main">
        {view === "chat" ? (
          <>
            <header className="desktop-header">
              <div><h1>Trò chuyện</h1><p>Tin nhắn riêng tư trong mạng nội bộ</p></div>
              <div className={`desktop-presence ${connected ? "connected" : ""}`}><i />{connected ? "iPhone đã kết nối" : "Đang chờ iPhone"}</div>
            </header>
            <section className="desktop-chat-layout">
              <div className="chat-column">
                <div className="conversation-label"><div className="phone-avatar">⌕</div><div><strong>iPhone của bạn</strong><span>{connected ? "Đang hoạt động" : "Chưa kết nối"}</span></div><span className={`online-dot ${connected ? "visible" : ""}`} /></div>
                <div className="desktop-messages">
                  {hasOlder && <button type="button" className="load-older" onClick={() => void loadOlder()}>Tải tin nhắn cũ hơn</button>}
                  {messages.length === 0 ? (
                    <div className="desktop-empty"><div className="empty-illustration"><div className="bubble-one">Hi!</div><div className="bubble-two">⌁</div><span>✦</span></div><h2>Chào mừng đến với ChatLink</h2><p>Gửi tin nhắn đầu tiên để bắt đầu cuộc trò chuyện với iPhone của bạn.</p></div>
                  ) : messages.map((message) => (
                    <article className={`desktop-message ${message.sender === "windows" ? "mine" : "theirs"}`} key={message.id}>
                      <div className="desktop-message-bubble"><p>{message.content}</p><footer><time>{formatTime(message.createdAt)}</time>{message.sender === "windows" && <span>{message.status === "delivered" ? "✓✓" : "✓"}</span>}</footer></div>
                    </article>
                  ))}
                  <div ref={bottomRef} />
                </div>
                {error && <div className="desktop-error">{error}</div>}
                <form className="desktop-composer" onSubmit={submitMessage}>
                  <textarea value={draft} onChange={(event) => setDraft(event.target.value)} maxLength={8192} rows={1} placeholder="Viết tin nhắn…" aria-label="Tin nhắn" onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }} />
                  <div className="composer-actions"><span>Enter để gửi · Shift + Enter xuống dòng</span><button type="submit" disabled={!draft.trim() || snapshot.status !== "online"}>Gửi <span>↗</span></button></div>
                </form>
                <div className="desktop-privacy"><span>⌑</span> Tin nhắn được lưu cục bộ trên máy tính này</div>
              </div>
              <aside className="connect-panel">
                <div className="panel-heading"><div><span className="panel-overline">KẾT NỐI THIẾT BỊ</span><h2>{snapshot.pairedDevice ? "iPhone đã ghép đôi" : "Thêm iPhone"}</h2></div><span className="panel-step">01</span></div>
                <p className="panel-copy">{certificateInstalled ? "Mở camera iPhone và quét mã để mở ChatLink." : "Quét mã để tải profile CA. Sau khi bật tin cậy trên iPhone, xác nhận bên dưới."}</p>
                <div className="qr-frame">
                  {qrUrl ? <QRCodeSVG value={qrUrl} size={166} level="M" includeMargin bgColor="#ffffff" fgColor="#273255" /> : <div className="qr-placeholder">Đang chuẩn bị<br />địa chỉ LAN…</div>}
                </div>
                <div className="connect-url-label">ĐỊA CHỈ CHATLINK</div>
                <div className="connect-url">{snapshot.httpsUrl?.replace(/\/$/, "") ?? "Chưa có IP LAN"}</div>
                {!certificateInstalled && <button className="copy-link" onClick={() => void confirmCertificateSetup()} disabled={!snapshot.setupUrl}>Đã cài và bật tin cậy chứng chỉ<span>✓</span></button>}
                <div className="connect-divider"><span />{snapshot.pairedDevice ? "THIẾT BỊ ĐÃ GHÉP ĐÔI" : "MÃ GHÉP ĐÔI"}<span /></div>
                {snapshot.pairedDevice ? <p className="paired-note">{snapshot.pairedDevice} tự kết nối khi mở lại ChatLink.</p> : <>
                  <div className="access-code" aria-label={`Mã ghép đôi ${snapshot.pairingCode ?? "chưa có"}`}>{codeGroups.map((digit, index) => <span className={index === 3 ? "split" : ""} key={`${index}-${digit}`}>{digit}</span>)}</div>
                  <p className="code-hint">Nhập mã trên iPhone. Mã hết hạn sau 10 phút{snapshot.pairingExpiresAt ? `, lúc ${formatTime(snapshot.pairingExpiresAt)}` : ""}.</p>
                </>}
                <div className="certificate-note"><div className="certificate-icon">⌑</div><div><strong>{certificateInstalled ? "Đã xác nhận cài chứng chỉ" : "Cài chứng chỉ HTTPS"}</strong><span>{certificateInstalled ? "Nếu đổi IP máy tính, hãy quét QR mới và thêm biểu tượng từ Safari." : "Trước lần kết nối đầu, tải profile và bật tin cậy trên iPhone."}</span></div></div>
              <button className="copy-link" onClick={() => void copySetupUrl()} disabled={!(certificateInstalled ? snapshot.httpsUrl : snapshot.setupUrl)}>{copied ? "Đã sao chép liên kết" : certificateInstalled ? "Sao chép địa chỉ ChatLink" : "Sao chép link cài chứng chỉ"}<span>⧉</span></button>
              </aside>
            </section>
            {snapshot.error && <div className="status-banner">{snapshot.error}</div>}
          </>
        ) : (
          <section className="settings-view">
            <header className="desktop-header"><div><h1>Cài đặt</h1><p>Quản lý server LAN và dữ liệu ChatLink</p></div><div className={`desktop-presence ${snapshot.status === "online" ? "connected" : ""}`}><i />{snapshot.status === "online" ? "Server đang chạy" : "Server chưa sẵn sàng"}</div></header>
            <div className="settings-card"><div className="settings-title"><span className="settings-symbol">⌁</span><div><h2>Server LAN</h2><p>Địa chỉ mà iPhone dùng để kết nối</p></div></div>
              <label className="settings-label">Interface mạng
                <select value={snapshot.selectedIp ?? ""} onChange={(event) => void changeInterface(event.target.value)} disabled={switching || interfaces.length === 0}>
                  {interfaces.length === 0 ? <option value="">Không tìm thấy IPv4 riêng</option> : interfaces.map((option) => <option key={option.ip} value={option.ip}>{option.label}</option>)}
                </select>
              </label>
              <div className="settings-row"><span>HTTPS và WebSocket</span><strong>{snapshot.selectedIp ? `${snapshot.selectedIp}:42100` : "Chưa hoạt động"}</strong></div>
              <div className="settings-row"><span>Profile cài CA</span><strong>{snapshot.setupUrl ?? "Không khả dụng"}</strong></div>
              <div className="settings-row fingerprint-row"><span>SHA-256 fingerprint</span><code>{snapshot.caFingerprint || "Đang tạo…"}</code></div>
              <div className="settings-row"><span>Trạng thái iPhone</span><strong>{connected ? "Đã kết nối" : "Đang chờ"}</strong></div>
              <div className="settings-row"><span>Thiết bị ghép đôi</span><strong>{snapshot.pairedDevice ?? "Chưa có"}</strong></div>
              {snapshot.pairedDevice && <button className="secondary-button unpair-button" onClick={() => void unpairIphone()}>Hủy ghép đôi iPhone</button>}
            </div>
            <div className="settings-card data-card"><div className="settings-title"><span className="settings-symbol">▤</span><div><h2>Dữ liệu cục bộ</h2><p>Lịch sử chat và chứng chỉ được giữ trên máy này.</p></div></div><button className="secondary-button" onClick={() => void invoke("open_data_folder")}>Mở thư mục dữ liệu <span>↗</span></button></div>
            <div className="settings-card"><div className="settings-title"><span className="settings-symbol">ⓘ</span><div><h2>Kết nối iPhone</h2><p>Chỉ dùng trên mạng riêng của bạn.</p></div></div>
              <p>Cho phép ChatLink qua Windows Firewall trên <strong>Private network</strong>. iPhone cần cùng Wi-Fi và tin cậy chứng chỉ ChatLink.</p>
              <p>Khi IP đổi, chọn interface mới và quét QR mới. Do Safari lưu token theo URL, hãy hủy ghép đôi iPhone cũ trên Windows, rồi nhập mã ghép đôi mới tại URL mới. Xuất outbox pending ở URL cũ trước khi hủy ghép đôi và nhập lại ở URL mới. Sau đó thêm lại biểu tượng Home Screen; lịch sử đã lưu trên Windows sẽ đồng bộ lại.</p>
            </div>
            {error && <div className="desktop-error settings-error">{error}</div>}
          </section>
        )}
      </main>
    </div>
  );
}

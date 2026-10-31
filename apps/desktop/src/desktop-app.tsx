import { useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";

type Snapshot = {
  status: string;
  interfaces: string[];
  selectedIp: string | null;
  httpUrl: string | null;
  mdnsUrl: string | null;
  hostnameUrl: string | null;
  pairedDevice: string | null;
  pairedDevices: Array<{ id: string; name: string }>;
  pendingPairing: { deviceId: string; name: string } | null;
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
  status: "starting",
  interfaces: [],
  selectedIp: null,
  httpUrl: null,
  mdnsUrl: null,
  hostnameUrl: null,
  pairedDevice: null,
  pairedDevices: [],
  pendingPairing: null,
  iphoneStatus: "waiting",
  error: null,
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
  const qrUrl = snapshot.httpUrl ?? snapshot.mdnsUrl;

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
      setError(String(reason));
    }
  }

  async function handlePairingDecision(approve: boolean) {
    if (!snapshot.pendingPairing) return;
    const deviceId = snapshot.pendingPairing.deviceId;
    try {
      const nextStatus = await invoke<Snapshot>("approve_pairing", { deviceId, approve });
      setSnapshot(nextStatus);
    } catch (reason) {
      setError(String(reason));
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

  async function copyUrl(url: string | null) {
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setError("Không thể sao chép liên kết.");
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
    if (!window.confirm("Hủy ghép đôi thiết bị? Lịch sử trên Windows vẫn được giữ lại.")) return;
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
        <div className="desktop-brand">
          <div className="brand-mark">C</div>
          <div><strong>ChatLink V2</strong><span>ZERO-TOUCH LAN</span></div>
        </div>
        <div className="workspace-label">WORKSPACE</div>
        <button className={`nav-item ${view === "chat" ? "active" : ""}`} onClick={() => setView("chat")}>
          <span className="nav-icon">◉</span> Trò chuyện <span className="nav-count">1</span>
        </button>
        <button className={`nav-item ${view === "settings" ? "active" : ""}`} onClick={() => setView("settings")}>
          <span className="nav-icon">⚙</span> Cài đặt
        </button>
        <div className="sidebar-spacer" />
        <div className="host-card">
          <div className="host-avatar">W</div>
          <div className="host-name"><strong>Máy tính này</strong><span>Windows host</span></div>
          <span className={`tiny-status ${snapshot.status === "online" ? "online" : "offline"}`} />
        </div>
        <div className="sidebar-version">ChatLink V2 · Zero-Touch LAN</div>
      </aside>

      <main className="desktop-main">
        {view === "chat" ? (
          <>
            <header className="desktop-header">
              <div>
                <h1>Trò chuyện</h1>
                <p>Tin nhắn riêng tư trong mạng Wi-Fi nội bộ</p>
              </div>
              <div className={`desktop-presence ${connected ? "connected" : ""}`}>
                <i />{connected ? "iPhone đã kết nối" : "Đang chờ iPhone"}
              </div>
            </header>

            {/* 1-Touch Approval Alert Banner */}
            {snapshot.pendingPairing && (
              <div className="pairing-alert-card">
                <span className="pairing-alert-badge">YÊU CẦU GHÉP ĐÔI</span>
                <h3>Thiết bị mới muốn kết nối</h3>
                <p>
                  <strong>{snapshot.pendingPairing.name}</strong> (ID: {snapshot.pendingPairing.deviceId.substring(0, 16)}...)
                </p>
                <div className="pairing-alert-actions">
                  <button className="approve-button" onClick={() => void handlePairingDecision(true)}>
                    ✓ Cho phép kết nối vĩnh viễn
                  </button>
                  <button className="reject-button" onClick={() => void handlePairingDecision(false)}>
                    ✕ Từ chối
                  </button>
                </div>
              </div>
            )}

            <section className="desktop-chat-layout">
              <div className="chat-column">
                <div className="conversation-label">
                  <div className="phone-avatar">⌕</div>
                  <div>
                    <strong>{snapshot.pairedDevice ?? "iPhone của bạn"}</strong>
                    <span>{connected ? "Đang hoạt động" : "Chưa kết nối"}</span>
                  </div>
                  <span className={`online-dot ${connected ? "visible" : ""}`} />
                </div>

                <div className="desktop-messages">
                  {hasOlder && (
                    <button type="button" className="load-older" onClick={() => void loadOlder()}>
                      Tải tin nhắn cũ hơn
                    </button>
                  )}
                  {messages.length === 0 ? (
                    <div className="desktop-empty">
                      <div className="empty-illustration">
                        <div className="bubble-one">Hi!</div>
                        <div className="bubble-two">⌁</div>
                        <span>✦</span>
                      </div>
                      <h2>Chào mừng đến với ChatLink V2</h2>
                      <p>Mở iPhone cùng Wi-Fi để bắt đầu nhắn tin tức thì. Không cần cài chứng chỉ CA.</p>
                    </div>
                  ) : (
                    messages.map((message) => (
                      <article className={`desktop-message ${message.sender === "windows" ? "mine" : "theirs"}`} key={message.id}>
                        <div className="desktop-message-bubble">
                          <p>{message.content}</p>
                          <footer>
                            <time>{formatTime(message.createdAt)}</time>
                            {message.sender === "windows" && <span>{message.status === "delivered" ? "✓✓" : "✓"}</span>}
                          </footer>
                        </div>
                      </article>
                    ))
                  )}
                  <div ref={bottomRef} />
                </div>

                {error && <div className="desktop-error">{error}</div>}

                <form className="desktop-composer" onSubmit={submitMessage}>
                  <textarea
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    maxLength={8192}
                    rows={1}
                    placeholder="Viết tin nhắn…"
                    aria-label="Tin nhắn"
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        event.currentTarget.form?.requestSubmit();
                      }
                    }}
                  />
                  <div className="composer-actions">
                    <span>Enter để gửi · Shift + Enter xuống dòng</span>
                    <button type="submit" disabled={!draft.trim() || snapshot.status !== "online"}>
                      Gửi <span>↗</span>
                    </button>
                  </div>
                </form>
                <div className="desktop-privacy"><span>⌑</span> Tin nhắn lưu an toàn trên máy tính này</div>
              </div>

              <aside className="connect-panel">
                <div className="panel-heading">
                  <div>
                    <span className="panel-overline">KẾT NỐI ZERO-TOUCH</span>
                    <h2>{snapshot.pairedDevice ? "Đã ghép đôi" : "Quét để kết nối"}</h2>
                  </div>
                  <span className="panel-step">V2</span>
                </div>
                <p className="panel-copy">
                  Mở Camera iPhone quét mã QR bên dưới để mở ChatLink trong Safari, sau đó chọn <strong>Chia sẻ ➔ Thêm vào Màn hình chính</strong>.
                </p>

                <div className="qr-frame">
                  {qrUrl ? (
                    <QRCodeSVG value={qrUrl} size={166} level="M" includeMargin bgColor="#ffffff" fgColor="#273255" />
                  ) : (
                    <div className="qr-placeholder">Đang chuẩn bị<br />địa chỉ LAN…</div>
                  )}
                </div>

                <div className="connect-method">
                  <div className="connect-method-title">ĐỊA CHỈ TRUY CẬP TRÊN IPHONE</div>
                  <div className="connect-method-desc">Gõ vào Safari hoặc mở qua QR:</div>
                  <div className="url-pills">
                    {snapshot.mdnsUrl && (
                      <div className="url-pill">
                        <strong>{snapshot.mdnsUrl}</strong>
                        <span>mDNS Khuyên dùng</span>
                      </div>
                    )}
                    {snapshot.hostnameUrl && (
                      <div className="url-pill">
                        <strong>{snapshot.hostnameUrl}</strong>
                        <span>Tên máy</span>
                      </div>
                    )}
                    {snapshot.httpUrl && (
                      <div className="url-pill">
                        <strong>{snapshot.httpUrl}</strong>
                        <span>IP trực tiếp</span>
                      </div>
                    )}
                  </div>
                </div>

                <button
                  className="copy-link"
                  onClick={() => void copyUrl(snapshot.mdnsUrl ?? snapshot.httpUrl)}
                  disabled={!snapshot.httpUrl && !snapshot.mdnsUrl}
                >
                  {copied ? "Đã sao chép liên kết" : "Sao chép địa chỉ mDNS"}
                  <span>⧉</span>
                </button>

                <div className="connect-divider">
                  <span />
                  THIẾT BỊ
                  <span />
                </div>

                {snapshot.pairedDevice ? (
                  <p className="paired-note">
                    {snapshot.pairedDevice} sẽ tự động kết nối mỗi khi bạn mở app trên iPhone!
                  </p>
                ) : (
                  <p className="code-hint">
                    Chưa có thiết bị nào. Khi iPhone kết nối lần đầu, một thông báo duyệt 1 chạm sẽ hiển thị ở đây.
                  </p>
                )}
              </aside>
            </section>
            {snapshot.error && <div className="status-banner">{snapshot.error}</div>}
          </>
        ) : (
          <section className="settings-view">
            <header className="desktop-header">
              <div>
                <h1>Cài đặt</h1>
                <p>Quản lý server LAN và thiết bị đã ghép đôi</p>
              </div>
              <div className={`desktop-presence ${snapshot.status === "online" ? "connected" : ""}`}>
                <i />{snapshot.status === "online" ? "Server đang chạy" : "Server chưa sẵn sàng"}
              </div>
            </header>

            <div className="settings-card">
              <div className="settings-title">
                <span className="settings-symbol">⌁</span>
                <div>
                  <h2>Server LAN HTTP</h2>
                  <p>Cổng kết nối nội bộ cho iPhone</p>
                </div>
              </div>
              <label className="settings-label">Interface mạng
                <select
                  value={snapshot.selectedIp ?? ""}
                  onChange={(event) => void changeInterface(event.target.value)}
                  disabled={switching || interfaces.length === 0}
                >
                  {interfaces.length === 0 ? (
                    <option value="">Không tìm thấy IPv4 riêng</option>
                  ) : (
                    interfaces.map((option) => (
                      <option key={option.ip} value={option.ip}>{option.label}</option>
                    ))
                  )}
                </select>
              </label>

              <div className="settings-row">
                <span>Cổng dịch vụ HTTP & WebSocket</span>
                <strong>42100 (Không cần HTTPS)</strong>
              </div>
              <div className="settings-row">
                <span>Tên miền mDNS cục bộ</span>
                <strong>{snapshot.mdnsUrl ?? "Đang phát sóng..."}</strong>
              </div>
              <div className="settings-row">
                <span>Tên máy tính nội bộ</span>
                <strong>{snapshot.hostnameUrl ?? "Chưa rõ"}</strong>
              </div>
              <div className="settings-row">
                <span>Trạng thái iPhone</span>
                <strong>{connected ? "Đã kết nối" : "Đang chờ kết nối"}</strong>
              </div>
              <div className="settings-row">
                <span>Thiết bị đã ghép đôi</span>
                <strong>{snapshot.pairedDevice ?? "Chưa có"}</strong>
              </div>

              {snapshot.pairedDevice && (
                <button className="secondary-button unpair-button" onClick={() => void unpairIphone()}>
                  Hủy ghép đôi iPhone
                </button>
              )}
            </div>

            <div className="settings-card data-card">
              <div className="settings-title">
                <span className="settings-symbol">▤</span>
                <div>
                  <h2>Dữ liệu cục bộ</h2>
                  <p>Lịch sử chat được lưu trong cơ sở dữ liệu SQLite trên máy này.</p>
                </div>
              </div>
              <button className="secondary-button" onClick={() => void invoke("open_data_folder")}>
                Mở thư mục dữ liệu <span>↗</span>
              </button>
            </div>

            <div className="settings-card">
              <div className="settings-title">
                <span className="settings-symbol">ⓘ</span>
                <div>
                  <h2>Nguyên lý hoạt động V2</h2>
                  <p>Tiện dụng tối đa · Zero-Touch Auto-Connect</p>
                </div>
              </div>
              <p>1. Cả máy tính và iPhone chỉ cần cùng kết nối vào một mạng Wi-Fi.</p>
              <p>2. Khi mở icon ChatLink trên iPhone, điện thoại tự động phân giải tên miền <code>chatlink.local</code> để tìm máy tính mà không bị ảnh hưởng bởi việc đổi IP của router.</p>
              <p>3. Khi mở khóa màn hình iPhone, app tự động khôi phục kết nối và đồng bộ tin nhắn ngay lập tức.</p>
            </div>

            {error && <div className="desktop-error settings-error">{error}</div>}
          </section>
        )}
      </main>
    </div>
  );
}

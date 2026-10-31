# ChatLink V2 — Đề xuất thiết kế hệ thống LAN Chat Windows ↔ iPhone (Tiện dụng tối đa)

> **Mục tiêu phiên bản:** Giải quyết triệt để vấn đề mất kết nối, quy trình cài đặt rườm rà, bẫy IP động (DHCP), và độ trễ phục hồi kết nối. Đạt trải nghiệm **Zero-Touch Auto-Connect**: mở app máy tính, mở app điện thoại (cùng Wi-Fi) là tự động kết nối và chat được ngay.

---

## 1. Bối cảnh và Mục tiêu V2

### 1.1. Bối cảnh & Rào cản
- **Không phân phối qua App Store:** Ứng dụng không đưa lên App Store, không có Apple Developer Enterprise, không sử dụng giải pháp sideload có hạn 7 ngày (AltStore/Sideloadly).
- **Môi trường kết nối duy nhất:** Máy tính Windows và iPhone cùng kết nối vào một mạng Wi-Fi nội bộ (LAN). Không cần Internet, không cần máy chủ trung gian Cloud/VPS.
- **Vấn đề cốt lõi của V1:** 
  1. Ép buộc dùng HTTPS tự ký (Self-signed Root CA + profile `.mobileconfig`), gây ra quy trình cài đặt chứng chỉ 5 bước phức tạp trên iOS.
  2. Bị trói buộc vào IP tĩnh (`https://192.168.x.x:42100`). Khi DHCP của router đổi IP máy tính, icon PWA trên iPhone bị chết link, mất toàn bộ Origin storage (IndexedDB, token) và phải ghép đôi lại từ đầu.
  3. Thiếu cơ chế phát hiện dịch vụ (Service Discovery) tự động.
  4. Cơ chế ghép đôi mong manh, dễ tự hủy token xác thực khi có lỗi mạng tạm thời.

### 1.2. Mục tiêu trọng tâm của V2
1. **Thiết lập lần đầu siêu tốc (Zero Setup Friction):** Không cài đặt profile CA, không mở Cài đặt iOS để bật Full Trust. Người dùng chỉ cần mở trình duyệt/quét QR và bấm "Thêm vào Màn hình chính" (Add to Home Screen).
2. **Kháng đổi IP 100% (DHCP Immunity):** Địa chỉ máy tính thay đổi không làm hỏng kết nối, không làm mất lịch sử hay token trên iPhone nhờ tên miền cục bộ **mDNS (`chatlink.local`)**.
3. **Tự động kết nối tức thì (Zero-Touch Auto-Connect):** Khi bật máy tính và mở app trên điện thoại, hai bên tự tìm thấy nhau và hoàn tất kết nối trong **< 500ms** mà không cần bất kỳ thao tác thủ công nào.
4. **Phục hồi kết nối siêu nhạy (Instant Reconnect on Focus):** Khi người dùng mở khóa màn hình iPhone hoặc chuyển lại ứng dụng, WebSocket tự động khôi phục và đồng bộ tin nhắn ngay lập tức.
5. **Cơ chế ghép đôi 1 chạm bền vững (One-Touch Persistent Trust):** Ghép đôi 1 lần duy nhất trên máy tính, lưu token vĩnh viễn, không bao giờ tự xóa thông tin xác thực khi rớt mạng.

---

## 2. So sánh Kiến trúc: V1 vs V2

| Đặc tính | ChatLink V1 (Cũ) | ChatLink V2 (Mới) | Lợi ích V2 |
| :--- | :--- | :--- | :--- |
| **Giao thức mạng** | HTTPS + WSS (Port 42100, 42101) | **HTTP + WS thuần** (Port 42100) | Loại bỏ hoàn toàn hạ tầng chứng chỉ CA rườm rà. |
| **Cài chứng chỉ iOS** | Tải `.mobileconfig`, bật Full Trust | **Không cần chứng chỉ** | Giảm từ 8 bước xuống 1 bước mở link. |
| **Định danh máy chủ** | IP cố định (`192.168.x.x`) | **mDNS (`chatlink.local`)** + Fallback IP | Đổi IP Wi-Fi vẫn kết nối bình thường. |
| **Bảo toàn dữ liệu** | Đổi IP = Mất sạch dữ liệu Origin | **Origin cố định** `http://chatlink.local:42100` | Token và lịch sử IndexedDB vĩnh viễn không mất. |
| **Quy trình ghép đôi** | Gõ mã 6 số (hạn 10 phút, tự khóa) | **1 chạm xác nhận trên PC**, lưu vĩnh viễn | Thuận tiện, không lo hết hạn hay gõ nhầm. |
| **Cơ chế Reconnect** | Backoff lũy thừa (chờ tới 10s) | **Lập tức (Active Reconnect on Focus)** | Mở app là có kết nối ngay lập tức (<500ms). |
| **Xử lý lỗi Auth** | Gặp lỗi là xóa sạch token mobile | **Giữ token**, retry khi mạng ổn định | Không bao giờ bị văng ra màn hình nhập mã. |

---

## 3. Kiến trúc Tổng thể V2

```text
                             MẠNG WI-FI / LAN NỘI BỘ
                             
┌────────────────────────────────────────────────────────────────────────┐
│                        MÁY TÍNH WINDOWS                                │
│                     (ChatLink Desktop App)                             │
│                                                                        │
│  ┌────────────────────────┐         ┌───────────────────────────────┐  │
│  │      Desktop UI        │         │        Rust Core Service      │  │
│  │   React + TypeScript   │◄───────►│  - HTTP Web Server (Port 42100)│  │
│  └────────────────────────┘         │  - WebSocket Hub              │  │
│                                     │  - mDNS Responder             │  │
│                                     │    (Phát tên 'chatlink.local')│  │
│                                     │  - Pairing & Auth Manager     │  │
│                                     │  - SQLite Engine (chatlink.db)│  │
│                                     └──────────────┬────────────────┘  │
└────────────────────────────────────────────────────┼───────────────────┘
                                                     │
                                                     │ HTTP / WebSocket (ws://)
                                                     │ Phân giải qua mDNS: chatlink.local
                                                     │
┌────────────────────────────────────────────────────┼───────────────────┐
│                           IPHONE                   │                   │
│               (Màn hình chính - WebClip App)       ▼                   │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Giao diện WebClip (Standalone Mode, không thanh địa chỉ Safari) │  │
│  │  - React UI + Virtualized Message List                           │  │
│  │  - Realtime WebSocket Client                                     │  │
│  │  - Auto Discovery Engine (mDNS Primary + Subnet Fallback)        │  │
│  │  - Lifecycle Watcher (Bắt sự kiện mở khóa / Active lại app)       │  │
│  │  - Local Storage / IndexedDB (Lưu token & Outbox tin nhắn)       │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Chi tiết các Thành phần Kỹ thuật

### 4.1. Khám phá Dịch vụ qua mDNS (ZeroConf / Bonjour)

#### Nguyên lý:
- iOS tích hợp sẵn mDNS (Bonjour). Khi Safari hoặc hệ thống mạng iOS thấy một hostname kết thúc bằng `.local`, nó tự động gửi gói tin multicast DNS (UDP port 5353) để tìm IP của máy tính trong mạng Wi-Fi.
- Ứng dụng Windows Desktop sẽ tích hợp một mDNS Responder (sử dụng crate Rust như `mdns-sd` hoặc tương đương) để đăng ký:
  - Hostname: `chatlink.local` trỏ tới IP LAN hiện tại của máy tính.
  - Service: `_chatlink._tcp.local` port `42100`.

#### Miễn nhiễm với IP động:
1. Máy tính đổi IP từ `192.168.1.15` sang `192.168.1.45`.
2. Rust mDNS Responder phát hiện card mạng thay đổi và cập nhật lại bản ghi A Record cho `chatlink.local`.
3. iPhone chỉ kết nối tới `http://chatlink.local:42100`. Do đó:
   - URL không đổi.
   - Origin không đổi (`http://chatlink.local:42100`).
   - Icon trên Home Screen luôn mở đúng app.
   - Toàn bộ dữ liệu trong IndexedDB và Token xác thực vẫn nguyên vẹn.

#### Cơ chế Fallback (Khi Router bật AP Isolation / Chặn Multicast):
Một số router Wi-Fi đặc thù chặn gói tin UDP Multicast giữa các thiết bị con:
- **Tầng 1 (Primary):** Thử kết nối `http://chatlink.local:42100`.
- **Tầng 2 (Cache Fallback):** Thử IP thành công gần nhất được lưu trong `localStorage` (ví dụ `http://192.168.1.15:42100`).
- **Tầng 3 (Manual / QR):** Màn hình Desktop luôn có một mã QR chứa địa chỉ IP hiện tại để quét tức thì nếu mạng Wi-Fi chặn hoàn toàn mDNS.

---

### 4.2. Giao thức Web & Tối ưu WebClip cho iPhone

#### Loại bỏ HTTPS:
- Trình duyệt Safari trên iOS hỗ trợ WebSocket (`ws://`) trên các kết nối HTTP nội bộ mạng LAN mà không hiện bất kỳ cảnh báo nguy hiểm nào.
- Bỏ hoàn toàn: file `.mobileconfig`, Root CA, certificate thumbprint, HTTPS listener, cổng setup `42101`.
- Chỉ duy nhất 1 cổng dịch vụ: **Port 42100** phục vụ cả Web tĩnh và WebSocket.

#### Trải nghiệm Native qua Apple WebClip Tags:
Trong file `index.html` của mobile app, cấu hình các thẻ meta tiêu chuẩn của Apple:
```html
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
<meta name="apple-mobile-web-app-title" content="ChatLink">
<link rel="apple-touch-icon" href="/icons/apple-touch-icon.png">
```
Khi người dùng bấm **Share ➔ Add to Home Screen**:
- Ứng dụng mở ra toàn màn hình (Standalone Mode), **không có thanh URL hay thanh điều hướng của Safari**.
- Vuốt chạm, thanh cuộn và bàn phím hoạt động mượt mà như một ứng dụng Native viết bằng Swift.

---

### 4.3. Quy trình Ghép đôi & Xác thực (Pairing & Trust)

#### Triết lý V2: "One-Touch Approval, Permanent Trust"
Không dùng mã số 6 chữ số có hạn 10 phút. Thay vào đó, dùng cơ chế ghép đôi 1 chạm:

```text
iPhone                                                   Windows PC
  │                                                           │
  │── 1. Gửi pairing_request (deviceId, friendlyName, secret) ──►│
  │                                                           │
  │                                     ┌───────────────────────────────────┐
  │                                     │ Hiện thông báo popup trên Desktop:│
  │                                     │ "Yêu cầu kết nối từ: iPhone       │
  │                                     │  [Từ chối]       [Cho phép]       │
  │                                     └─────────────────┬─────────────────┘
  │                                                       │
  │                                              Người dùng bấm [Cho phép]
  │                                                       │
  │                                     - Lưu deviceId và secret_hash vào DB
  │                                     - Tạo token phiên dài hạn
  │                                                       │
  │◄── 2. Trả về pairing_ok (token, serverId) ────────────┘
  │
  ├─ Lưu token vào IndexedDB/localStorage
  │
  ▼
ĐÃ GHÉP ĐÔI THÀNH CÔNG (VĨNH VIỄN)
```

#### Các lần mở app tiếp theo (Auto-Connect):
1. iPhone mở app, đọc `deviceId` và `token` từ local storage.
2. Mở kết nối WebSocket tới `ws://chatlink.local:42100/ws`.
3. Gửi ngay gói tin đầu tiên:
   ```json
   {
     "type": "auth",
     "payload": {
       "deviceId": "iphone-uuid-xyz",
       "token": "secret-auth-token"
     }
   }
   ```
4. Máy tính kiểm tra hash trong SQLite:
   - Nếu hợp lệ: Trả về `auth_ok` kèm sequence number tin nhắn mới nhất.
   - Toàn bộ quá trình hoàn tất trong **dưới 100ms**.
5. **Quy tắc an toàn dữ liệu:** Nếu kết nối thất bại vì máy tính đang tắt hoặc mất Wi-Fi, **tuyệt đối KHÔNG xóa token**. Ứng dụng chỉ hiển thị trạng thái "Đang tìm máy tính..." và tự thử lại.

---

### 4.4. Chiến lược Phục hồi Kết nối Tức thì (Instant Reconnect Strategy)

Vấn đề lớn nhất của Safari PWA là khi khóa máy, iOS ngắt kết nối WebSocket để tiết kiệm pin.

#### Giải pháp V2: Đón đầu vòng đời ứng dụng (Lifecycle Hooking):
Thay vì phụ thuộc vào vòng lặp chờ thụ động (polling/backoff), app Mobile đăng ký các sự kiện hệ thống:
```typescript
// 1. Khi người dùng mở khóa màn hình hoặc chuyển lại app
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    triggerInstantReconnect();
  }
});

// 2. Khi trang web được Safari khôi phục từ bộ nhớ đệm BFCache
window.addEventListener("pageshow", (event) => {
  triggerInstantReconnect();
});

// 3. Khi thiết bị kết nối lại Wi-Fi
window.addEventListener("online", () => {
  triggerInstantReconnect();
});
```

#### Hàm `triggerInstantReconnect()`:
1. Kiểm tra trạng thái WebSocket hiện tại: nếu đã đứt hoặc đang `CONNECTING` quá 1 giây, chủ động `close()` và khởi tạo WebSocket mới ngay lập tức.
2. Không chờ delay: lần thử đầu tiên diễn ra tại `t = 0ms`.
3. Nếu chưa thành công (ví dụ PC vừa khởi động): thử lại với khoảng cách ngắn: `500ms`, `1000ms`, `2000ms` (tối đa 3s một lần, không để người dùng chờ lâu).
4. Ngay khi `auth_ok`, gửi ngay `sync_request` để lấy các tin nhắn gửi từ máy tính trong lúc điện thoại khóa màn hình.

---

### 4.5. Cơ chế Đồng bộ & Chống Trùng lặp Tin nhắn

#### Nhận diện tin nhắn:
- Mỗi tin nhắn được tạo kèm một định danh duy nhất dạng **ULID** (Universally Unique Lexicographically Sortable Identifier).
- ULID vừa đảm bảo tính duy nhất toàn cầu, vừa sắp xếp được theo thời gian tạo.

#### Luồng gửi tin (Outbox Pattern):
```text
[Người dùng gõ tin & Bấm Gửi]
       │
       ▼
1. Lưu tin vào Local Outbox (IndexedDB) với status = "pending"
2. Hiển thị ngay lên màn hình chat (Optimistic UI)
       │
       ▼
Có kết nối WebSocket?
   ├── KHÔNG: Giữ nguyên trong Outbox, gửi lại ngay khi có kết nối.
   └── CÓ: Gửi gói tin chat_message qua WebSocket
            │
            ▼
Máy tính nhận tin ➔ Lưu vào SQLite ➔ Cấp số sequence (seq)
            │
            ▼
Máy tính gửi message_ack { id: "ulid", seq: 124, createdAt: "..." }
            │
            ▼
iPhone nhận ACK ➔ Cập nhật status = "sent" ➔ Xóa khỏi Outbox
```

---

## 5. Cấu trúc Cơ sở Dữ liệu (SQLite trên Windows)

Ứng dụng desktop giữ nguyên sự đơn giản với SQLite thông qua `sqlx`, lưu trữ tại thư mục Application Data an toàn:

### Bảng `devices` (Thiết bị đã ghép đôi)
```sql
CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,            -- UUID của thiết bị
    friendly_name TEXT NOT NULL,    -- Ví dụ: "iPhone của Hoàng Hà"
    token_hash TEXT NOT NULL,       -- SHA-256 của secret token
    created_at TEXT NOT NULL,       -- Thời điểm ghép đôi
    last_seen_at TEXT NOT NULL      -- Lần kết nối gần nhất
);
```

### Bảng `messages` (Kho lưu trữ tin nhắn trung tâm)
```sql
CREATE TABLE IF NOT EXISTS messages (
    seq INTEGER PRIMARY KEY AUTOINCREMENT, -- Thứ tự tăng dần phục vụ sync
    id TEXT UNIQUE NOT NULL,               -- ULID của tin nhắn
    sender TEXT NOT NULL,                  -- 'windows' hoặc 'iphone'
    content TEXT NOT NULL,                 -- Nội dung văn bản
    status TEXT NOT NULL,                  -- 'stored', 'delivered'
    created_at TEXT NOT NULL               -- ISO 8601 UTC
);

CREATE INDEX IF NOT EXISTS idx_messages_seq ON messages(seq);
CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
```

### Bảng `settings` (Cấu hình ứng dụng)
```sql
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

---

## 6. Định dạng Giao thức WebSocket V2

Tất cả các gói tin trao đổi qua WebSocket đều tuân theo chuẩn JSON Envelope:
```json
{
  "type": "string",
  "requestId": "string (tùy chọn)",
  "timestamp": "ISO-8601 string",
  "payload": {}
}
```

### Danh mục gói tin:
1. `auth`: Điện thoại gửi thông tin xác thực (`deviceId`, `token`).
2. `auth_ok`: Máy tính chấp nhận xác thực, trả về `maxSeq` hiện tại của hệ thống.
3. `auth_fail`: Lỗi xác thực (sai token).
4. `pairing_request`: Điện thoại xin kết nối lần đầu (`deviceId`, `friendlyName`, `secret`).
5. `pairing_ok`: Người dùng máy tính đã bấm cho phép.
6. `pairing_reject`: Người dùng máy tính bấm từ chối.
7. `chat_message`: Gửi tin nhắn mới (`content`, `id`).
8. `message_ack`: Xác nhận máy chủ đã lưu tin vào SQLite thành công.
9. `delivered`: Báo nhận tin khi đối phương đã đọc/nhận được.
10. `sync_request`: Yêu cầu tải tin nhắn mới kể từ `afterSeq`.
11. `sync_batch`: Gửi danh sách tin nhắn đồng bộ.
12. `ping` / `pong`: Giữ nhịp kết nối định kỳ 15 giây.

---

## 7. Giao diện & Trải nghiệm Người dùng (UX Workflow)

### 7.1. Trải nghiệm lần đầu tiên (First-time Onboarding - Chỉ mất 30 giây)
1. **Trên máy tính:** Mở `ChatLink.exe`. Màn hình hiển thị:
   - Trạng thái: "Đang phát sóng tại `http://chatlink.local:42100`".
   - Một mã QR to rõ ràng ở giữa màn hình.
2. **Trên iPhone:**
   - Mở Camera quét mã QR (hoặc gõ `http://chatlink.local:42100` vào Safari).
   - Trang web mở lên ngay lập tức (không có bất kỳ cảnh báo bảo mật nào).
   - Safari hiện gợi ý ngắn: *"Bấm nút Chia sẻ ➔ Thêm vào Màn hình chính"*.
   - Người dùng bấm Thêm vào Màn hình chính.
3. **Mở icon ChatLink trên iPhone:**
   - App tự gửi yêu cầu kết nối.
   - Trên máy tính hiện popup: *"iPhone muốn kết nối với bạn. Bấm [Đồng ý]"*.
   - Người dùng bấm **[Đồng ý]**.
   - Màn hình chuyển sang giao diện chat. **Hoàn tất thiết lập vĩnh viễn!**

### 7.2. Trải nghiệm hàng ngày (Daily Usage - 0 thao tác)
- Người dùng ngồi vào máy tính mở ChatLink.
- Cầm iPhone bấm vào icon ChatLink trên màn hình chính.
- Biểu tượng mạng chuyển sang chấm xanh **"Đã kết nối" trong vòng 0.3s**.
- Người dùng gõ tin và nhắn ngay lập tức.
- Khóa màn hình iPhone ➔ Mở lại iPhone: kết nối tự nhảy lại màu xanh và nhận các tin nhắn mới gửi từ PC.

---

## 8. Kế hoạch Triển khai (Migration & Implementation Plan)

### Giai đoạn 1: Dọn dẹp & Tái cấu trúc Backend Rust (Desktop)
1. Xóa bỏ module chứng chỉ `certs.rs` và các phụ thuộc liên quan đến DPAPI, x509, mobileconfig.
2. Cập nhật `server.rs`: Đơn giản hóa Axum server chỉ chạy HTTP thuần trên cổng `42100`.
3. Tích hợp thư viện mDNS (`mdns-sd`) vào `server.rs` / `lib.rs` để quảng bá tên miền `chatlink.local`.
4. Viết lại cơ chế ghép đôi: Chuyển từ mã 6 số sang danh sách `paired_devices` với sự phê duyệt 1 chạm từ Desktop UI.

### Giai đoạn 2: Cải tiến Frontend Mobile & Giao thức Kết nối
1. Cập nhật `index.html` của mobile app: Tinh chỉnh WebClip meta tags để chạy full màn hình không viền trình duyệt.
2. Viết lại cơ chế kết nối trong `mobile-chat.tsx`:
   - Ưu tiên kết nối hostname `chatlink.local:42100`.
   - Bắt các sự kiện `visibilitychange` và `pageshow` để khôi phục kết nối tức thì.
   - Loại bỏ đoạn mã tự xóa token `clearCredentials()` khi mất kết nối.

### Giai đoạn 3: Cập nhật Desktop UI & Kiểm thử Thực tế
1. Thiết kế lại bảng thông báo kết nối trên Desktop UI (popup duyệt ghép đôi thiết bị mới).
2. Kiểm tra thực tế trên iPhone và máy tính cùng mạng Wi-Fi:
   - Thử nghiệm tắt/bật Wi-Fi trên cả 2 thiết bị.
   - Thử nghiệm khởi động lại router để đổi dải IP (kiểm chứng tính kháng DHCP của mDNS).
   - Đo thời gian phục hồi kết nối khi mở khóa màn hình iPhone.

---

## 9. Tiêu chí Nghiệm thu (Definition of Done)

- [ ] Không còn bất kỳ file `.mobileconfig` hay cảnh báo cài chứng chỉ nào.
- [ ] iPhone truy cập được vào `http://chatlink.local:42100` qua trình duyệt Safari mà không cần nhập địa chỉ IP thủ công.
- [ ] Thêm được app ra Màn hình chính của iOS với icon chuẩn và mở lên ở chế độ toàn màn hình không có thanh URL.
- [ ] Máy tính đổi địa chỉ IP nhưng iPhone vẫn tự kết nối lại thành công mà không cần quét lại mã hay ghép đôi lại.
- [ ] Thời gian tự kết nối lại khi mở khóa màn hình iPhone dưới 1 giây.
- [ ] Tin nhắn gửi đi khi mất kết nối được lưu trong outbox và tự động gửi bù khi có mạng trở lại.


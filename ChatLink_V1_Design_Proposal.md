# ChatLink V1 — Đề xuất thiết kế hệ thống LAN Chat Windows ↔ iPhone

## 1. Mục tiêu

**ChatLink V1** là một công cụ chat cá nhân 1–1 giữa:

- **Máy tính Windows** chạy ứng dụng desktop `ChatLink.exe`.
- **iPhone** chạy ChatLink dưới dạng **PWA (Progressive Web App)** được thêm vào Home Screen.

V1 chỉ hoạt động khi **Windows và iPhone cùng kết nối vào một mạng LAN/Wi‑Fi**.

Mục tiêu chính:

- Gửi và nhận tin nhắn văn bản theo thời gian thực.
- Độ trễ thấp trong mạng LAN.
- Không cần Internet.
- Không cần App Store.
- Không cần Apple Developer Account.
- Không cần VPS, cloud backend hoặc database server.
- Không có giới hạn 7 ngày như app iOS sideload bằng tài khoản Apple miễn phí.
- Thiết kế nhỏ gọn, dễ bảo trì, không over-engineering.

---

## 2. Phạm vi V1

### 2.1. Chức năng có trong V1

- Chat văn bản 1–1 giữa Windows và iPhone.
- Kết nối realtime bằng WebSocket.
- Desktop app tự chạy server nội bộ.
- iPhone kết nối trực tiếp đến IP LAN của máy Windows.
- Pair thiết bị bằng mã pairing.
- Lưu lịch sử tin nhắn trên máy Windows bằng SQLite.
- Hiển thị trạng thái:
  - Connecting
  - Connected
  - Disconnected
- Tự động reconnect khi mất Wi‑Fi tạm thời.
- Timestamp cho mỗi tin nhắn.
- Trạng thái gửi:
  - Pending
  - Sent
  - Delivered
- Đồng bộ lại tin nhắn sau khi reconnect.
- PWA có icon riêng trên Home Screen iPhone.
- Giao diện responsive cho iPhone.

### 2.2. Chưa có trong V1

- Tailscale.
- Chat qua Internet/4G/5G.
- Cloud server.
- Firebase.
- Push Notification khi iPhone đóng app.
- Gửi ảnh.
- Gửi file.
- Voice message.
- Group chat.
- Multi-user account.
- Login Google/Apple.
- End-to-End Encryption riêng ở tầng ứng dụng.
- Đồng bộ nhiều máy tính.
- Message reactions.
- Delete/Edit message.
- Video/audio call.

Các chức năng trên có thể xem xét ở V2 hoặc các phiên bản sau.

---

# 3. Kiến trúc tổng thể

```text
                         LOCAL WI-FI / LAN

┌──────────────────────────────────────────────────────────────┐
│                         Windows PC                           │
│                                                              │
│                      ChatLink.exe                            │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                    Desktop UI                          │  │
│  │              React + TypeScript                       │  │
│  └────────────────────────┬───────────────────────────────┘  │
│                           │                                  │
│                           ▼                                  │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                 Tauri / Rust Core                      │  │
│  │                                                        │  │
│  │  - HTTP Server                                         │  │
│  │  - WebSocket Server                                    │  │
│  │  - Pairing Service                                     │  │
│  │  - Message Service                                     │  │
│  │  - Device Session                                      │  │
│  │  - SQLite Repository                                   │  │
│  └────────────────────────┬───────────────────────────────┘  │
│                           │                                  │
│                           ▼                                  │
│                      chatlink.db                             │
└───────────────────────────┬──────────────────────────────────┘
                            │
                            │ HTTP + WebSocket
                            │
                            │ LAN / Wi-Fi
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│                           iPhone                             │
│                                                              │
│                    ChatLink PWA                              │
│                                                              │
│  React + TypeScript                                         │
│  Service Worker                                             │
│  WebSocket Client                                           │
│  IndexedDB                                                  │
│                                                              │
│                 Add to Home Screen                           │
└──────────────────────────────────────────────────────────────┘
```

---

# 4. Công nghệ đề xuất

## 4.1. Windows Desktop

| Thành phần | Công nghệ |
|---|---|
| Desktop framework | Tauri 2 |
| UI | React |
| Language UI | TypeScript |
| Native/Core | Rust |
| Local database | SQLite |
| Realtime protocol | WebSocket |
| HTTP server | Rust HTTP server |
| Build | Tauri CLI |

### Vì sao dùng Tauri

Tauri phù hợp với ChatLink vì:

- Nhẹ hơn Electron.
- File `.exe` nhỏ hơn.
- Không đóng gói một Chromium hoàn chỉnh.
- Rust phù hợp để chạy HTTP/WebSocket server ổn định.
- Có thể truy cập filesystem và SQLite dễ dàng.
- Phù hợp với utility desktop chạy lâu dài.

---

## 4.2. iPhone

| Thành phần | Công nghệ |
|---|---|
| UI | React |
| Language | TypeScript |
| Build tool | Vite |
| App model | PWA |
| Realtime | WebSocket |
| Local cache | IndexedDB |
| Offline support | Service Worker |

iPhone không cần `.ipa`.

Người dùng chỉ cần:

```text
Safari
  ↓
http://<PC-IP>:<PORT>
  ↓
Share
  ↓
Add to Home Screen
  ↓
ChatLink
```

Sau đó ChatLink xuất hiện như một icon app riêng trên Home Screen.

---

# 5. Mô hình triển khai V1

Máy Windows là **host/server duy nhất**.

Ví dụ:

```text
Windows LAN IP:
192.168.1.15

ChatLink HTTP:
http://192.168.1.15:42100

ChatLink WebSocket:
ws://192.168.1.15:42101
```

iPhone cùng Wi‑Fi truy cập:

```text
http://192.168.1.15:42100
```

Sau khi mở lần đầu và pair thành công, người dùng thêm vào Home Screen.

---

# 6. Nguyên tắc kết nối

## 6.1. Server ownership

Windows luôn đóng vai trò:

```text
SERVER
```

iPhone luôn đóng vai trò:

```text
CLIENT
```

Không triển khai peer-to-peer hai chiều ở tầng transport.

Điều này giúp:

- kiến trúc đơn giản;
- dễ quản lý trạng thái;
- dễ lưu lịch sử;
- dễ xử lý reconnect;
- không cần discovery phức tạp.

---

# 7. Luồng khởi động Windows App

Khi người dùng chạy:

```text
ChatLink.exe
```

ứng dụng thực hiện:

```text
1. Khởi tạo SQLite
        ↓
2. Load cấu hình
        ↓
3. Xác định địa chỉ IPv4 LAN
        ↓
4. Start HTTP Server
        ↓
5. Start WebSocket Server
        ↓
6. Load paired device
        ↓
7. Hiển thị Desktop UI
```

Nếu tất cả thành công:

```text
Server status: Online
LAN address: 192.168.1.15
iPhone: Waiting
```

---

# 8. Device Discovery

## 8.1. V1 đơn giản

V1 chưa cần mDNS/Bonjour.

Windows hiển thị:

```text
Connect your iPhone

http://192.168.1.15:42100
```

kèm QR Code.

Ví dụ:

```text
┌───────────────────────────────┐
│        Connect iPhone         │
│                               │
│        ███████████            │
│        ██ QR CODE █           │
│        ███████████            │
│                               │
│ http://192.168.1.15:42100     │
│                               │
└───────────────────────────────┘
```

iPhone scan QR để mở URL.

### Lợi ích

- đơn giản;
- ổn định;
- không phụ thuộc Bonjour;
- không cần network discovery daemon;
- dễ debug.

### V2 có thể thêm

```text
chatlink.local
```

bằng mDNS.

---

# 9. Pairing

## 9.1. Mục tiêu

Không cho bất kỳ thiết bị nào trong Wi‑Fi tự động vào chat.

## 9.2. Luồng pairing

Windows tạo mã:

```text
593 281
```

iPhone mở ChatLink lần đầu:

```text
Enter pairing code

[ 593 281 ]

[ Pair Device ]
```

Luồng:

```text
iPhone
   │
   │ pairing_request
   ▼
Windows
   │
   │ verify code
   ▼
Generate device token
   │
   ▼
iPhone
```

Sau đó iPhone lưu token trong IndexedDB.

Windows lưu:

```text
device_id
device_name
token_hash
paired_at
last_seen_at
```

---

# 10. Authentication V1

Sau khi pairing:

```text
WebSocket connect
        ↓
AUTH message
        ↓
device_id + token
        ↓
server verify
        ↓
AUTH_OK
```

Ví dụ:

```json
{
  "type": "auth",
  "deviceId": "iphone-main",
  "token": "<device-token>"
}
```

Server không lưu raw token.

Server chỉ lưu:

```text
SHA-256(token)
```

hoặc hash tương đương.

---

# 11. WebSocket Protocol

ChatLink sử dụng JSON message envelope.

## 11.1. Base format

```json
{
  "type": "message_type",
  "requestId": "uuid",
  "timestamp": "2026-09-24T01:00:00.000Z",
  "payload": {}
}
```

---

# 12. Message Types V1

## 12.1. auth

Client → Server

```json
{
  "type": "auth",
  "payload": {
    "deviceId": "iphone-main",
    "token": "secret-token"
  }
}
```

---

## 12.2. auth_ok

Server → Client

```json
{
  "type": "auth_ok",
  "payload": {
    "deviceId": "iphone-main"
  }
}
```

---

## 12.3. chat_message

Ví dụ iPhone gửi:

```json
{
  "type": "chat_message",
  "requestId": "01KABCDEFG",
  "timestamp": "2026-09-24T01:05:10.000Z",
  "payload": {
    "content": "Hello PC"
  }
}
```

---

## 12.4. message_ack

Server phản hồi:

```json
{
  "type": "message_ack",
  "payload": {
    "messageId": "01KABCDEFG",
    "status": "stored"
  }
}
```

---

## 12.5. delivered

```json
{
  "type": "delivered",
  "payload": {
    "messageId": "01KABCDEFG"
  }
}
```

---

## 12.6. ping / pong

```json
{
  "type": "ping"
}
```

```json
{
  "type": "pong"
}
```

Dùng để:

- phát hiện connection chết;
- cập nhật trạng thái thiết bị;
- hỗ trợ reconnect.

---

# 13. Luồng gửi tin từ iPhone

```text
User
 ↓
Type message
 ↓
React UI
 ↓
Create message ID
 ↓
Store local pending message
 ↓
WebSocket.send()
 ↓
Windows Server
 ↓
Validate
 ↓
Save SQLite
 ↓
message_ack
 ↓
iPhone
 ↓
Pending → Sent
```

Windows Desktop UI nhận message ngay sau khi server lưu thành công.

---

# 14. Luồng gửi tin từ Windows

```text
Windows UI
 ↓
Message Service
 ↓
SQLite
 ↓
WebSocket
 ↓
iPhone
 ↓
Client ACK
 ↓
Windows status:
Sent → Delivered
```

Nếu iPhone offline:

```text
Message stored in SQLite
status = pending_delivery
```

Khi iPhone reconnect:

```text
sync
 ↓
send pending messages
 ↓
delivered
```

---

# 15. Offline / Reconnect Strategy

## 15.1. iPhone mất Wi‑Fi

Client chuyển:

```text
CONNECTED
    ↓
DISCONNECTED
```

UI hiển thị:

```text
● Offline
```

Client reconnect theo backoff:

```text
1s
2s
4s
8s
10s
10s
10s...
```

Giới hạn tối đa:

```text
10 giây
```

---

## 15.2. Tin nhắn khi mất kết nối

Tin nhắn được lưu vào local outbox:

```text
IndexedDB
```

Ví dụ:

| ID | Content | State |
|---|---|---|
| M101 | Hello | pending |
| M102 | Test | pending |

Sau khi reconnect:

```text
AUTH
 ↓
SYNC
 ↓
flush outbox
 ↓
ACK
```

---

# 16. Đồng bộ tin nhắn

Mỗi message có ID duy nhất.

Khuyến nghị:

```text
ULID
```

thay vì auto increment cho giao thức.

Ví dụ:

```text
01K8QFNV3GAC2BN2D8AX7A0FQZ
```

Lợi ích:

- unique;
- sortable theo thời gian;
- sinh được ở cả client và server.

---

# 17. SQLite Schema

## 17.1. devices

```sql
CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    paired_at TEXT NOT NULL,
    last_seen_at TEXT
);
```

---

## 17.2. messages

```sql
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    sender_device_id TEXT NOT NULL,
    content TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    delivered_at TEXT
);
```

---

## 17.3. app_settings

```sql
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Không tạo thêm bảng nếu V1 chưa có nhu cầu thực tế.

---

# 18. Message State

Tin nhắn sử dụng state:

```text
PENDING
   ↓
STORED
   ↓
DELIVERED
```

Mapping UI:

```text
Pending      ○
Stored       ✓
Delivered    ✓✓
```

V1 chưa cần:

```text
READ
```

Có thể bổ sung trong V1.1 nếu cần.

---

# 19. Desktop UI

## 19.1. Main screen

```text
┌───────────────────────────────────────────────┐
│ ChatLink                         ● Connected │
├───────────────────────────────────────────────┤
│                                               │
│                    Hello                      │
│                            01:10 ✓✓            │
│                                               │
│  Hi                                           │
│  01:10                                        │
│                                               │
│                    Test LAN                   │
│                            01:11 ✓✓            │
│                                               │
├───────────────────────────────────────────────┤
│ Message...                              Send  │
└───────────────────────────────────────────────┘
```

---

## 19.2. Settings

```text
ChatLink Settings

Server
LAN IP: 192.168.1.15
HTTP Port: 42100
WebSocket Port: 42101

Device
iPhone
Status: Paired

[Unpair Device]

[Open Data Folder]
```

---

# 20. iPhone UI

PWA sử dụng mobile-first UI.

```text
┌───────────────────────────┐
│ ChatLink        ● PC      │
├───────────────────────────┤
│                           │
│             Hello         │
│             01:10 ✓✓      │
│                           │
│ Hi                        │
│ 01:10                     │
│                           │
│             Test LAN      │
│             01:11 ✓✓      │
│                           │
├───────────────────────────┤
│ Message...            ➤   │
└───────────────────────────┘
```

---

# 21. PWA Manifest

Ví dụ:

```json
{
  "name": "ChatLink",
  "short_name": "ChatLink",
  "start_url": "/",
  "display": "standalone",
  "orientation": "portrait",
  "background_color": "#ffffff",
  "theme_color": "#ffffff",
  "icons": [
    {
      "src": "/icons/icon-192.png",
      "sizes": "192x192",
      "type": "image/png"
    },
    {
      "src": "/icons/icon-512.png",
      "sizes": "512x512",
      "type": "image/png"
    }
  ]
}
```

---

# 22. Local Storage trên iPhone

Không dùng LocalStorage cho message queue chính.

Dùng:

```text
IndexedDB
```

Các object store:

```text
settings
messages
outbox
```

Ví dụ:

```text
settings
 ├── device_id
 └── auth_token

messages
 └── cached messages

outbox
 └── messages chưa gửi
```

---

# 23. Hiệu năng

Vì V1 chạy hoàn toàn trong LAN:

```text
iPhone
  │
  │ Wi-Fi
  ▼
Router / Access Point
  │
  ▼
Windows
```

không có:

- DNS Internet;
- cloud server;
- external API;
- remote database.

Với tin nhắn text, payload rất nhỏ.

Ví dụ:

```json
{
  "type": "chat_message",
  "payload": {
    "content": "hello"
  }
}
```

Thông thường chỉ vài trăm byte.

WebSocket giữ connection sống lâu dài, do đó không cần HTTP handshake cho từng message.

---

# 24. Mục tiêu hiệu năng V1

Trong điều kiện Wi‑Fi bình thường:

| Chỉ số | Mục tiêu |
|---|---:|
| UI send feedback | < 50 ms |
| LAN message delivery | < 200 ms |
| SQLite insert | < 20 ms |
| Reconnect detection | < 5 s |
| Cold app startup | < 2 s |

Các giá trị trên là **mục tiêu thiết kế**, không phải SLA.

---

# 25. Network Interface

Windows có thể có nhiều interface:

```text
Ethernet
Wi-Fi
VPN
Loopback
VirtualBox
Docker
```

V1 ưu tiên IPv4 private address:

```text
192.168.x.x
10.x.x.x
172.16.x.x - 172.31.x.x
```

Loại bỏ:

```text
127.0.0.1
169.254.x.x
```

Nếu phát hiện nhiều interface hợp lệ:

```text
Select network interface
```

có thể được thêm trong Settings.

---

# 26. Port

Khuyến nghị:

```text
HTTP:
42100

WebSocket:
42101
```

Hoặc có thể sử dụng cùng một port nếu Rust HTTP framework hỗ trợ WebSocket upgrade.

Ưu tiên cuối cùng:

```text
42100
```

cho cả:

```text
HTTP
WebSocket Upgrade
```

để giảm cấu hình firewall.

---

# 27. Windows Firewall

Lần đầu ChatLink chạy server, Windows có thể yêu cầu:

```text
Allow ChatLink to communicate on:

☑ Private networks
☐ Public networks
```

Chỉ nên cho phép:

```text
Private networks
```

Không yêu cầu Public network access cho V1.

---

# 28. Security Model V1

V1 giả định:

```text
Trusted Home/Private LAN
```

Nhưng vẫn áp dụng các lớp cơ bản:

1. Device Pairing.
2. Token authentication.
3. Token không lưu raw trên Windows.
4. Reject unknown WebSocket clients.
5. Không expose ra Internet.
6. Windows Firewall chỉ Private Network.
7. Input validation.
8. Message length limit.

---

# 29. Message Length Limit

Đề xuất V1:

```text
Maximum text message:
8 KB
```

Nếu vượt:

```text
MESSAGE_TOO_LARGE
```

Điều này giúp tránh payload bất thường.

---

# 30. Rate Limit

Dù chỉ dùng cá nhân, server vẫn có giới hạn nhẹ:

```text
30 messages / 10 seconds
```

Không cần Redis.

Chỉ dùng in-memory counter.

---

# 31. Session

Mỗi lần iPhone kết nối:

```text
WebSocket Session
```

Server chỉ cho phép:

```text
1 active iPhone session
```

Nếu cùng device kết nối lại:

```text
old session
    ↓
close
    ↓
new session
```

Phù hợp với mô hình 1–1.

---

# 32. Server State

Server cần giữ:

```text
current_device
current_socket
connection_status
last_seen
```

Không cần distributed state.

---

# 33. Project Structure

```text
chatlink/
│
├── apps/
│   │
│   ├── desktop/
│   │   ├── src/
│   │   │   ├── components/
│   │   │   ├── pages/
│   │   │   ├── hooks/
│   │   │   ├── stores/
│   │   │   └── services/
│   │   │
│   │   └── src-tauri/
│   │       ├── src/
│   │       │   ├── server/
│   │       │   ├── websocket/
│   │       │   ├── pairing/
│   │       │   ├── message/
│   │       │   ├── database/
│   │       │   └── network/
│   │       │
│   │       └── migrations/
│   │
│   └── mobile-web/
│       ├── src/
│       │   ├── components/
│       │   ├── pages/
│       │   ├── services/
│       │   ├── db/
│       │   └── hooks/
│       │
│       ├── public/
│       │   ├── manifest.json
│       │   └── icons/
│       │
│       └── vite.config.ts
│
├── packages/
│   └── protocol/
│       ├── message-types.ts
│       └── schemas.ts
│
├── docs/
│   └── architecture.md
│
├── package.json
└── README.md
```

---

# 34. Shared Protocol Package

Không duplicate WebSocket schema giữa desktop và mobile.

Tạo:

```text
packages/protocol
```

Ví dụ:

```ts
export type ChatMessage = {
  type: "chat_message";
  requestId: string;
  timestamp: string;
  payload: {
    content: string;
  };
};
```

Có thể dùng:

```text
Zod
```

để validate message runtime.

---

# 35. Recommended Libraries

## Desktop frontend

```text
React
TypeScript
Vite
Zustand
```

Không cần Redux.

---

## Rust backend

Có thể dùng:

```text
tokio
axum
sqlx
serde
serde_json
uuid / ulid
sha2
```

### Vai trò

```text
tokio
→ async runtime

axum
→ HTTP + WebSocket

sqlx
→ SQLite

serde
→ JSON serialize/deserialize

ulid
→ message ID

sha2
→ token hash
```

---

## iPhone PWA

```text
React
TypeScript
Vite
idb
Zod
```

Không cần framework nặng hơn.

---

# 36. State Management

Desktop và iPhone chỉ cần:

```text
connection
messages
device
settings
```

Zustand store ví dụ:

```text
useChatStore
├── messages
├── connectionState
├── sendMessage()
├── receiveMessage()
└── retryPending()
```

---

# 37. Logging

V1 log:

```text
logs/chatlink.log
```

Nội dung:

```text
server started
client connected
client authenticated
message stored
client disconnected
reconnect
database error
```

Không log:

```text
device token
raw secret
```

Có thể hạn chế log message content nếu muốn riêng tư hơn.

---

# 38. Data Location

Windows:

```text
%APPDATA%\ChatLink\
```

Ví dụ:

```text
C:\Users\<User>\AppData\Roaming\ChatLink\
```

Cấu trúc:

```text
ChatLink/
├── chatlink.db
├── config.json
└── logs/
    └── chatlink.log
```

---

# 39. Startup Behavior

V1:

```text
User manually starts ChatLink.exe
```

Chưa bật auto-start cùng Windows mặc định.

Có thể thêm option:

```text
[ ] Start ChatLink with Windows
```

trong V1.1.

---

# 40. Server Availability

Do Windows là server:

```text
Windows OFF
→ Chat unavailable

ChatLink.exe closed
→ Chat unavailable
```

Đây là giới hạn chấp nhận được của V1.

---

# 41. Error Handling

Các lỗi chính:

```text
SERVER_NOT_FOUND
AUTH_REQUIRED
AUTH_FAILED
PAIRING_CODE_INVALID
MESSAGE_TOO_LARGE
DATABASE_ERROR
CONNECTION_LOST
```

UI không hiển thị raw Rust error.

Ví dụ:

```text
Unable to connect to your PC.
Make sure both devices are on the same Wi‑Fi.
```

---

# 42. Pairing Reset

Windows:

```text
Settings
 ↓
Unpair iPhone
```

Server:

```text
delete device token
close socket
```

iPhone lần sau phải pair lại.

---

# 43. PWA Update Strategy

Mobile web assets được Windows app phục vụ.

Khi Windows được nâng cấp:

```text
ChatLink.exe V1.1
```

PWA assets cũng được cập nhật.

Service Worker dùng strategy:

```text
App Shell
→ cache-first

API/WebSocket
→ network-only
```

---

# 44. Không cache WebSocket data bằng Service Worker

Service Worker chỉ dùng cho:

```text
HTML
CSS
JS
icons
app shell
```

Messages được lưu bằng:

```text
IndexedDB
```

Không dùng Cache API cho message history.

---

# 45. Development Mode

Trong development:

```text
Desktop React:
localhost:1420

Rust server:
localhost:42100

Mobile dev:
localhost:5173
```

Khi test với iPhone:

```text
Vite host:
0.0.0.0
```

hoặc build mobile assets vào server Rust.

---

# 46. Production Mode

Khi build release:

```text
ChatLink.exe
```

sẽ chứa:

```text
Desktop frontend
+
Rust server
+
Mobile PWA static assets
```

Không cần chạy:

```text
npm server
node.exe
python
```

bên ngoài.

---

# 47. Build Output

Mục tiêu:

```text
ChatLink_1.0.0_x64-setup.exe
```

hoặc:

```text
ChatLink.exe
```

Tùy chiến lược packaging.

---

# 48. Quy trình sử dụng lần đầu

## Windows

```text
1. Cài ChatLink.
2. Mở ChatLink.
3. Cho phép Windows Firewall trên Private Network.
4. Mở màn hình Pair Device.
5. QR Code xuất hiện.
```

## iPhone

```text
1. Kết nối cùng Wi-Fi với Windows.
2. Scan QR.
3. Safari mở ChatLink.
4. Nhập Pairing Code.
5. Pair thành công.
6. Add to Home Screen.
7. Mở ChatLink từ icon.
```

Hoàn tất.

---

# 49. Quy trình sử dụng hằng ngày

```text
Windows
↓
Open ChatLink.exe

iPhone
↓
Tap ChatLink

Connected
↓
Chat
```

Không cần login lại.

---

# 50. Các milestone triển khai

## Milestone 1 — Desktop foundation

- Tauri project.
- React desktop UI.
- Rust backend.
- Start local server.
- SQLite migration.
- Network IP detection.

### Kết quả

Windows chạy được `ChatLink.exe`.

---

## Milestone 2 — iPhone PWA

- React mobile UI.
- PWA manifest.
- Service Worker.
- Responsive chat screen.
- Connect LAN server.

### Kết quả

iPhone mở được ChatLink từ Windows server.

---

## Milestone 3 — WebSocket Chat

- WebSocket server.
- WebSocket client.
- chat_message.
- message_ack.
- reconnect.
- ping/pong.

### Kết quả

Windows ↔ iPhone chat realtime.

---

## Milestone 4 — Persistence

- SQLite messages.
- IndexedDB.
- message history.
- pending queue.
- reconnect sync.

### Kết quả

Không mất message khi reconnect.

---

## Milestone 5 — Pairing

- pairing code.
- device token.
- token hash.
- authenticated WebSocket.
- unpair.

### Kết quả

Chỉ iPhone đã pair mới truy cập được.

---

## Milestone 6 — Packaging

- production build.
- embed PWA assets.
- Windows installer.
- firewall documentation.
- smoke test.

### Kết quả

ChatLink V1 release.

---

# 51. Acceptance Criteria

V1 được xem là hoàn thành khi:

1. Windows cài và chạy ChatLink mà không cần Node.js cài riêng.
2. ChatLink tự xác định được LAN IPv4.
3. iPhone cùng Wi‑Fi truy cập được ChatLink.
4. iPhone có thể Add to Home Screen.
5. Windows ↔ iPhone gửi tin nhắn realtime.
6. Tin nhắn được lưu trên Windows.
7. Reload iPhone không mất lịch sử gần nhất.
8. Wi‑Fi mất rồi kết nối lại, app tự reconnect.
9. Pending messages được gửi lại.
10. Thiết bị chưa pair không vào được chat.
11. App không cần Internet để chat trong LAN.
12. Không cần App Store.
13. Không cần Apple Developer.
14. Không cần VPS hoặc cloud service.

---

# 52. Test Scenarios

## Test 1 — Normal Chat

```text
Windows ON
iPhone same Wi-Fi
↓
Send iPhone → Windows
↓
< 200 ms target
```

---

## Test 2 — Reverse Chat

```text
Windows → iPhone
```

Tin nhắn phải hiển thị realtime.

---

## Test 3 — Wi-Fi Disconnect

```text
iPhone disconnect Wi-Fi
↓
send message
↓
Pending
↓
reconnect Wi-Fi
↓
Auto reconnect
↓
message delivered
```

---

## Test 4 — Desktop Restart

```text
Close ChatLink
↓
Open ChatLink
↓
SQLite history restored
```

---

## Test 5 — iPhone Reload

```text
Close PWA
↓
Open again
↓
Reconnect
↓
History restored
```

---

## Test 6 — Unauthorized Device

Thiết bị khác trong cùng Wi‑Fi thử WebSocket connection.

Expected:

```text
AUTH_REQUIRED
```

và disconnect.

---

## Test 7 — Invalid Token

Expected:

```text
AUTH_FAILED
```

---

# 53. Những quyết định kiến trúc chính

## Quyết định 1

**Windows là server.**

Không sử dụng cloud intermediary.

---

## Quyết định 2

**WebSocket cho realtime.**

Không polling.

Không WebRTC.

---

## Quyết định 3

**SQLite cho server persistence.**

Không PostgreSQL.

Không MySQL.

Không Redis.

---

## Quyết định 4

**iPhone dùng PWA.**

Không Swift.

Không Flutter native.

Không `.ipa`.

---

## Quyết định 5

**LAN-only ở V1.**

Không Tailscale.

Không port forwarding.

Không public Internet.

---

## Quyết định 6

**Một paired device duy nhất.**

Không thiết kế user/account system.

---

# 54. Vì sao kiến trúc này phù hợp V1

ChatLink chỉ có bài toán:

```text
1 Windows
+
1 iPhone
+
text messages
+
same LAN
```

Do đó hệ thống không cần:

```text
Microservices
RabbitMQ
Kafka
Redis
PostgreSQL
Kubernetes
Docker
OAuth
Firebase
Cloud server
API Gateway
WebRTC
```

Các công nghệ trên sẽ làm tăng độ phức tạp mà không giải quyết thêm giá trị thiết yếu cho V1.

Kiến trúc:

```text
Tauri
+
React
+
Rust
+
SQLite
+
WebSocket
+
PWA
```

đã đủ cho yêu cầu hiện tại.

---

# 55. Giới hạn quan trọng của V1

## 55.1. Chỉ hoạt động cùng mạng

Ví dụ:

```text
Windows:
Home Wi-Fi

iPhone:
Home Wi-Fi

✓ works
```

Nhưng:

```text
Windows:
Home Wi-Fi

iPhone:
4G/5G

✗ V1 không hoạt động
```

V2 mới thêm Tailscale.

---

## 55.2. Windows phải bật

Do Windows là server:

```text
PC shutdown
→ server unavailable
```

---

## 55.3. iPhone background

V1 không triển khai Push Notification.

Khi PWA bị iOS suspend ở background:

```text
realtime socket có thể bị đóng
```

Khi người dùng mở lại ChatLink:

```text
WebSocket reconnect
↓
sync messages
```

Đây là hành vi chấp nhận trong V1.

---

# 56. Hướng phát triển V2

Sau khi V1 ổn định, V2 có thể thêm:

```text
Tailscale
```

Kiến trúc ứng dụng gần như không đổi.

Từ:

```text
iPhone
   │
LAN
   │
Windows
```

thành:

```text
iPhone
   │
Tailscale
   │
Windows
```

WebSocket protocol, SQLite, message model và UI vẫn được giữ nguyên.

Đây là lý do V1 nên thiết kế network layer độc lập với business/message layer ngay từ đầu.

---

# 57. Kết luận

ChatLink V1 được thiết kế theo nguyên tắc:

> **Một Windows desktop app làm host, một iPhone PWA làm client, giao tiếp trực tiếp qua LAN bằng WebSocket.**

Kiến trúc chính thức đề xuất:

```text
WINDOWS
Tauri 2
React
TypeScript
Rust
Axum
SQLite
WebSocket
       │
       │ LAN / Wi-Fi
       ▼
IPHONE
React
TypeScript
PWA
WebSocket
IndexedDB
```

Đây là phạm vi đủ nhỏ để triển khai nhanh, nhưng vẫn tạo nền tảng tốt cho V2 khi cần:

- Tailscale;
- Push Notification;
- gửi file;
- encryption riêng;
- nhiều device hơn.

**Ưu tiên của V1 là: đơn giản, nhanh, ổn định, riêng tư và không over-engineering.**

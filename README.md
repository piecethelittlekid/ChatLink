# ChatLink V2

**ChatLink** là giải pháp chat thời gian thực siêu tốc, an toàn và riêng tư giữa **Máy tính Windows** và **iPhone** trong mạng nội bộ (Wi-Fi LAN) mà **không cần thông qua Apple App Store** và **không cần qua bất kỳ server trung gian nào trên Internet**.

---

## 🌟 Tính năng nổi bật của ChatLink V2

- 🚀 **Kết nối tự động tức thì (Zero-touch Auto-Connect):** Chỉ cần mở app trên máy tính và chạm vào icon trên điện thoại, hai bên sẽ tự động bắt tay và vào trạng thái chat ngay lập tức.
- 🚫 **Xóa bỏ hoàn toàn gánh nặng chứng chỉ:** Không còn quy trình cài đặt Root Certificate Authority (CA) hay cài đặt profile cấu hình `.mobileconfig` phiền phức trên iOS. Hệ thống chạy trực tiếp giao thức WebSocket tốc độ cao trên cổng tiêu chuẩn `42100`.
- 🌐 **Độc lập với địa chỉ IP bằng mDNS (`chatlink.local`):** Ứng dụng tích hợp sẵn bộ phát đáp mDNS (Multicast DNS) thuần Rust. Dù Router Wi-Fi cấp lại địa chỉ IP mới (DHCP lease đổi), iPhone vẫn tự động tìm thấy PC qua địa chỉ cố định `http://chatlink.local:42100`.
- 👆 **Phê duyệt ghép đôi 1 chạm (1-Touch Approval):** Không cần nhập mã PIN 6 số thủ công. Lần đầu kết nối, iPhone chỉ cần bấm *"Yêu cầu kết nối"*, máy tính sẽ hiện thông báo xác nhận để bạn bấm **[Cho phép kết nối]**.
- 📱 **Trải nghiệm Apple WebClip như App Native:** Hỗ trợ tính năng WebClip toàn màn hình của iOS (`apple-mobile-web-app-capable`). Khi thêm ra Màn hình chính (Home Screen), ứng dụng mở lên không có thanh địa chỉ Safari, giao diện vuốt chạm tối ưu như một ứng dụng iOS gốc.
- 💾 **Lưu trữ tin nhắn bền bỉ & Ngoại tuyến:** Lịch sử trò chuyện được lưu trữ bằng SQLite (trên Desktop) và IndexedDB (trên iPhone). Khi mất mạng hoặc ra ngoài vùng phủ sóng Wi-Fi, tin nhắn gửi đi sẽ được lưu vào hàng đợi (Outbox) và tự động đồng bộ bù ngay khi có kết nối lại.

---

## 📋 Yêu cầu hệ thống

1. **Thiết bị:**
   - **Máy tính:** Windows 10 hoặc Windows 11 (đã cài đặt WebView2 Runtime - có sẵn mặc định trên Windows 10/11).
   - **Điện thoại:** iPhone chạy iOS 15.0 trở lên.
2. **Mạng kết nối:**
   - Cả máy tính và iPhone phải cùng kết nối vào **chung một mạng Wi-Fi** (hoặc PC cắm dây LAN chung dải mạng với Wi-Fi của iPhone).
3. **Môi trường phát triển (Chỉ cần nếu bạn muốn tự build source code):**
   - Node.js 20+ và npm.
   - Rust stable (MSVC target) kèm Microsoft C++ Build Tools.

---

## 📖 HƯỚNG DẪN CÀI ĐẶT VÀ SỬ DỤNG ĐẦY ĐỦ

### Bước 1: Khởi chạy ChatLink trên Máy tính Windows

1. **Khởi chạy ứng dụng:**
   - **Dành cho người dùng:** Mở trực tiếp tệp thực thi [`chatlink-desktop.exe`](file:///d:/Desktop/NHAP/Personal_Project/ChatLink/apps/desktop/src-tauri/target/release/chatlink-desktop.exe) (hoặc bộ cài đặt của ChatLink).
   - **Dành cho lập trình viên:** Mở terminal trong thư mục dự án và chạy:
     ```powershell
     npm.cmd run dev:desktop
     ```
2. **Quyền truy cập mạng (Windows Firewall):**
   - Ở lần chạy đầu tiên, hộp thoại bảo mật Windows Defender Firewall sẽ xuất hiện hỏi quyền truy cập mạng.
   - **Quan trọng:** Tích chọn vào mục **Private networks** (Mạng riêng tư / Mạng gia đình) và bấm **Allow access** (Cho phép truy cập).
3. **Màn hình chính của Desktop:**
   - Ứng dụng sẽ hiển thị mã QR kèm các địa chỉ kết nối LAN:
     - `http://chatlink.local:42100` (Địa chỉ mDNS khuyên dùng)
     - `http://<tên-máy-tính>.local:42100` (Hostname dự phòng)
     - `http://<ip-mạng-lan>:42100` (IP trực tiếp)

---

### Bước 2: Kết nối và Cài đặt trên iPhone (Chỉ làm 1 LẦN DUY NHẤT)

1. **Mở giao diện trên iPhone:**
   - Đảm bảo iPhone đã bật Wi-Fi và kết nối cùng mạng với máy tính.
   - Mở ứng dụng **Camera** trên iPhone và quét mã QR đang hiển thị trên màn hình máy tính $\rightarrow$ Chạm vào liên kết màu vàng để mở trong trình duyệt **Safari**.
   - *(Hoặc mở Safari và gõ trực tiếp `http://chatlink.local:42100` vào thanh địa chỉ).*
2. **Gửi yêu cầu ghép đôi (1-Touch Pairing):**
   - Trên màn hình Safari của iPhone, bạn sẽ thấy thông báo: *"Thiết bị chưa ghép đôi"*.
   - Bấm vào nút **"Yêu cầu kết nối"**.
3. **Xác nhận trên Máy tính Windows:**
   - Ngay lập tức, trên màn hình máy tính sẽ xuất hiện một thanh thông báo màu xanh:
     > 🔔 **iPhone yêu cầu ghép đôi**  
     > Thiết bị: **iPhone** muốn kết nối với máy tính này.  
     > `[Cho phép kết nối]` `[Từ chối]`
   - Bạn chỉ cần bấm vào nút **[Cho phép kết nối]**.
   - Ngay tức khắc, iPhone sẽ được xác thực thành công và tự động chuyển thẳng vào màn hình chat!

---

### Bước 3: Đưa ứng dụng ra Màn hình chính iPhone (Tạo WebClip App)

Để ChatLink có biểu tượng icon riêng trên iPhone và mở toàn màn hình (không bị chiếm diện tích bởi thanh địa chỉ Safari):

1. Trên màn hình chat Safari của iPhone, bấm vào nút **Chia sẻ (Share)** (biểu tượng hình vuông có mũi tên trỏ lên ở giữa thanh công cụ phía dưới).
2. Cuộn danh sách xuống và chọn dòng **"Thêm vào Màn hình chính"** (*Add to Home Screen*).
3. Đặt tên ứng dụng là **ChatLink** rồi bấm **Thêm** (*Add*) ở góc trên bên phải.
4. Giờ đây, trên màn hình chính của iPhone đã có icon **ChatLink** giống hệt một ứng dụng tải từ App Store!

---

### Bước 4: Trải nghiệm sử dụng hàng ngày (Tự động kết nối 100%)

Kể từ lần thiết lập đầu tiên, bạn không cần phải quét mã QR hay xác nhận lại nữa:

1. **Mỗi khi cần chat:**
   - Mở ứng dụng **ChatLink** trên máy tính Windows.
   - Bấm vào biểu tượng **ChatLink** trên màn hình chính iPhone.
   - Hai ứng dụng sẽ **tự động bắt tay và kết nối thành công trong tích tắc (< 200 ms)**. Bạn có thể bắt đầu nhắn tin qua lại ngay lập tức!
2. **Khi khóa màn hình hoặc chuyển sang ứng dụng khác:**
   - iOS có thể tạm dừng các kết nối chạy ngầm để tiết kiệm pin.
   - Tuy nhiên, ngay khi bạn mở lại icon ChatLink hoặc mở khóa màn hình, cơ chế `fast reconnect` thông minh sẽ lập tức tái kích hoạt kết nối WebSocket ở mốc $t = 0\text{ ms}$ và đồng bộ đầy đủ toàn bộ tin nhắn mới mà bạn chưa đọc.

---

## 🛠️ Quản lý thiết bị & Tiện ích mở rộng

- **Quản lý thiết bị:**
  - Trong mục **Cài đặt** trên Desktop, bạn có thể xem danh sách các thiết bị iPhone đã được ghép đôi.
  - Bạn có thể bấm **Hủy ghép đôi** (*Unpair*) đối với thiết bị không còn sử dụng.
- **Tìm kiếm tin nhắn:**
  - Nhập từ khóa vào ô tìm kiếm trên thanh tiêu đề để tra cứu nhanh toàn bộ lịch sử trò chuyện.
- **Dữ liệu cục bộ an toàn:**
  - Cơ sở dữ liệu SQLite được lưu trữ riêng trong thư mục dữ liệu ứng dụng của người dùng Windows. Bạn có thể bấm **Mở thư mục dữ liệu** trong giao diện Desktop để kiểm tra hoặc sao lưu tệp `chatlink.db`.

---

## 👨‍💻 Hướng dẫn cho Nhà phát triển (Developer Guide)

### Cấu trúc dự án Monorepo

```text
ChatLink/
├── apps/
│   ├── desktop/             # Ứng dụng Desktop chạy bằng Tauri v2 (Rust + React + Vite)
│   │   ├── src/             # Giao diện React Desktop
│   │   └── src-tauri/       # Backend Rust: HTTP Server, WebSocket, SQLite, mDNS responder
│   └── mobile-web/          # Ứng dụng Mobile Web & iOS WebClip (React + Vite + Workbox PWA)
│       ├── src/             # Giao diện Mobile Chat, IndexedDB client, Fast Reconnect hooks
│       └── tests/           # Kiểm thử tự động IndexedDB và PWA Shell
├── packages/
│   └── protocol/            # Gói chia sẻ chung: Định nghĩa Zod Schemas & Types giao thức chat
└── docs/
    └── VERIFICATION.md      # Tài liệu và kết quả kiểm thử tự động
```

### Các lệnh phát triển & kiểm tra chất lượng

1. **Cài đặt thư viện dependencies:**
   ```powershell
   npm.cmd install
   ```

2. **Kiểm tra kiểu dữ liệu (Typecheck toàn bộ workspaces):**
   ```powershell
   npm.cmd run check
   ```

3. **Chạy kiểm thử tự động Backend Rust:**
   ```powershell
   cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
   ```

4. **Chạy kiểm thử Mobile IndexedDB & PWA Offline:**
   ```powershell
   npm.cmd run test:storage
   ```

5. **Đóng gói bản phát hành (Production Build):**
   ```powershell
   npm.cmd run build
   ```
   *File thực thi độc lập sẽ được tạo tại:* `apps/desktop/src-tauri/target/release/chatlink-desktop.exe`.

---

## ❓ Xử lý sự cố thường gặp (Troubleshooting)

- **iPhone báo không tìm thấy máy chủ `chatlink.local`:**
  - Đảm bảo iPhone và máy tính đang cùng bắt chung một tên Wi-Fi (SSID).
  - Một số bộ định tuyến Wi-Fi văn phòng hoặc công cộng có bật tính năng *"AP Isolation"* (cách ly các thiết bị kết nối với nhau). Hãy kiểm tra và tắt tính năng này trên Router Wi-Fi nếu có.
  - Nếu router chặn gói tin Multicast mDNS, bạn hãy nhập địa chỉ hostname máy tính: `http://<tên-máy-tính>.local:42100` hoặc địa chỉ IP LAN trực tiếp hiển thị trên Desktop (ví dụ `http://192.168.1.15:42100`).
- **Cổng 42100 bị xung đột:**
  - Nếu có ứng dụng khác đang chiếm cổng `42100`, hãy tắt ứng dụng đó trước khi mở ChatLink.
- **Không gửi được tin nhắn khi mở lại app:**
  - Kiểm tra xem máy tính Windows có bị rơi vào trạng thái Sleep/Hibernate không. Khi máy tính hoạt động lại, iPhone sẽ tự động kết nối lại ngay lập tức.

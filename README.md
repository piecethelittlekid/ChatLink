# ChatLink V1

ChatLink là ứng dụng chat văn bản chạy trên máy Windows, kết nối riêng tư qua mạng LAN với một iPhone. Ứng dụng desktop phục vụ PWA qua HTTPS và WebSocket trên cùng địa chỉ IPv4.

## Yêu cầu

- Node.js 20 trở lên và npm (chỉ để build)
- Rust stable với target MSVC
- Microsoft C++ Build Tools, workload **Desktop development with C++**
- Microsoft Edge WebView2 Runtime

iPhone cần kết nối cùng Wi-Fi với máy tính. Ở lần thiết lập đầu, hãy cài profile CA cục bộ của ChatLink và bật tin cậy đầy đủ trong phần cài đặt chứng chỉ của iOS.

Khi Windows Firewall hỏi quyền truy cập mạng, chỉ cho phép ChatLink trên **mạng Private**. Không bật quyền trên mạng Public.

## Phát triển

```powershell
npm.cmd install
npm.cmd run build:protocol
npm.cmd run build:mobile
npm.cmd run dev:desktop
```

Để mở giao diện iPhone trong trình duyệt desktop khi phát triển:

```powershell
npm.cmd run dev:mobile
```

## Kiểm tra và đóng gói

```powershell
npm.cmd run build
```

Kiểm tra giao thức và giao diện bằng `npm.cmd run check`; kiểm tra IndexedDB, outbox và đồng bộ trên Chrome headless bằng `npm.cmd run test:storage`. Có thể đặt biến `CHATLINK_BROWSER` trỏ tới Chrome/Edge nếu trình duyệt nằm ở đường dẫn khác. Kiểm thử Rust chạy bằng `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` sau khi cài toolchain. Bản build Tauri tạo `.exe` và NSIS installer trong `apps/desktop/src-tauri/target/release/`.

Kết quả kiểm chứng Windows gần nhất và các bước còn cần iPhone thật nằm trong [docs/VERIFICATION.md](docs/VERIFICATION.md).

Bản Tauri đóng gói bản build mobile làm tài nguyên. SQLite và CA cục bộ được lưu trong thư mục dữ liệu riêng của ứng dụng; chọn **Mở thư mục dữ liệu** trong ChatLink để xem đường dẫn chính xác.

## Thiết lập iPhone lần đầu

1. Mở ứng dụng Windows và quét QR đang hiển thị.
2. So sánh fingerprint trên trang cài đặt với fingerprint trong ChatLink trên máy tính.
3. Tải profile, cài profile trong Settings của iPhone, rồi bật tin cậy đầy đủ tại **Settings → General → About → Certificate Trust Settings**.
4. Trên Windows, xác nhận đã cài và bật tin cậy chứng chỉ. Quét QR mới để mở ChatLink qua HTTPS trong Safari, sau đó thêm ứng dụng vào Home Screen.
5. Nhập mã ghép đôi sáu chữ số đang hiển thị trên Windows trong 10 phút. Sau khi ghép đôi, iPhone lưu token trong IndexedDB và tự xác thực ở những lần mở sau.

Sau khi xác nhận cài CA, cổng HTTP dùng để tải profile sẽ dừng. Trạng thái này được lưu để những lần mở ChatLink tiếp theo chỉ phục vụ HTTPS.

## Lịch sử, mất kết nối và đổi IP

- Tin nhắn được ghi vào SQLite trước khi server xác nhận `stored`. Desktop tải lịch sử từ SQLite theo trang.
- iPhone lưu lịch sử và tin chưa ACK trong IndexedDB. Sau khi mở lại hoặc Wi‑Fi trở về, ứng dụng đồng bộ từ server rồi gửi lại outbox bằng ID cũ để tránh tin lặp.
- Nếu IP Windows đổi, trước tiên mở biểu tượng cũ để **Xuất outbox** nếu còn tin pending. Chọn interface mới trong **Cài đặt** trên Windows, rồi chọn **Hủy ghép đôi iPhone** để cấp mã ghép đôi mới. Quét QR mới, ghép đôi tại URL mới, **Nhập outbox** nếu đã xuất và thêm lại biểu tượng Home Screen. Safari lưu token và IndexedDB theo URL, nên URL mới cần ghép đôi lại; lịch sử đã lưu trên Windows sẽ đồng bộ lại.
- Để thay iPhone, chọn **Hủy ghép đôi iPhone** trong desktop. Thao tác xóa token và ngắt phiên; lịch sử Windows được giữ.
- Chỉ chứng chỉ CA công khai được đưa vào profile iOS. Khóa riêng CA được bảo vệ bằng Windows DPAPI; nếu một trong hai tệp CA hỏng hoặc thiếu, ứng dụng báo lỗi và cần khôi phục cả cặp tệp từ cùng bản sao lưu.

## Xử lý lỗi thường gặp

- **Không có địa chỉ LAN:** kết nối Wi-Fi hoặc Ethernet, rồi mở lại ChatLink. Trong **Cài đặt**, chọn đúng IPv4 của mạng đang dùng nếu máy có nhiều interface.
- **Cổng 42100 bận:** đóng ứng dụng đang dùng cổng này rồi chọn lại interface hoặc mở lại ChatLink. Cổng 42101 chỉ dùng khi đang tải profile CA lần đầu.
- **SQLite không mở được:** kiểm tra quyền ghi và dung lượng của thư mục dữ liệu từ nút **Mở thư mục dữ liệu**; khôi phục `chatlink.db` từ bản sao lưu nếu tệp hỏng.
- **CA không mở được:** chạy ChatLink bằng cùng tài khoản Windows đã tạo CA. DPAPI không giải mã khóa riêng dưới tài khoản khác. Nếu mất hoặc hỏng một tệp CA, khôi phục cả `chatlink-root-ca.der` và `chatlink-root-ca.key.dpapi` từ cùng bản sao lưu của tài khoản đó.
- **iPhone không mở được HTTPS:** kiểm tra cùng Wi-Fi, quyền Windows Firewall trên mạng Private, địa chỉ IP hiện tại và trạng thái tin cậy đầy đủ của CA trong cài đặt iOS. Nếu IP đổi, làm lại các bước ghép đôi ở trên.

## Giới hạn V1

ChatLink chỉ hoạt động khi Windows đang mở ứng dụng và hai thiết bị cùng LAN. PWA có thể bị iOS dừng ở nền; khi mở lại, nó sẽ kết nối và đồng bộ. V1 không có push notification, gửi file hay truy cập qua Internet.

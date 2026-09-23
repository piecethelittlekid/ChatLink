# ChatLink

ChatLink là ứng dụng chat văn bản chạy trên máy Windows, kết nối riêng tư qua mạng LAN với một iPhone. Ứng dụng desktop phục vụ PWA qua HTTPS và WebSocket trên cùng địa chỉ IPv4.

## Yêu cầu

- Node.js 20 trở lên và npm
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

## Đóng gói

```powershell
npm.cmd run build
```

Bản Tauri đóng gói bản build mobile làm tài nguyên. SQLite và CA cục bộ được lưu trong thư mục dữ liệu riêng của ứng dụng; chọn **Mở thư mục dữ liệu** trong ChatLink để xem đường dẫn chính xác.

## Thiết lập iPhone lần đầu

1. Mở ứng dụng Windows và quét QR đang hiển thị.
2. So sánh fingerprint trên trang cài đặt với fingerprint trong ChatLink trên máy tính.
3. Tải profile, cài profile trong Settings của iPhone, rồi bật tin cậy đầy đủ tại **Settings → General → About → Certificate Trust Settings**.
4. Trên Windows, xác nhận đã cài và bật tin cậy chứng chỉ. Quét QR mới để mở ChatLink qua HTTPS trong Safari, sau đó thêm ứng dụng vào Home Screen.
5. Nhập mã truy cập sáu chữ số đang hiển thị trên Windows.

Sau khi xác nhận cài CA, cổng HTTP dùng để tải profile sẽ dừng. Trạng thái này được lưu để những lần mở ChatLink tiếp theo chỉ phục vụ HTTPS.

## Phạm vi hiện tại

- Tin nhắn được ghi vào SQLite trước khi server xác nhận `stored`.
- Tin iPhone chưa được xác nhận giữ trong bộ nhớ trang hiện tại; lịch sử bền vững trên iPhone và đồng bộ IndexedDB thuộc Milestone 4.
- Mã truy cập thay đổi mỗi khi mở ứng dụng desktop. Pairing lâu dài và device token thuộc Milestone 5.
- Chỉ chứng chỉ CA công khai được đưa vào profile iOS. Khóa riêng CA được bảo vệ bằng Windows DPAPI.

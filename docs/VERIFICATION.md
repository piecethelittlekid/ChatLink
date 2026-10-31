# Báo cáo Kiểm chứng ChatLink V2

Tài liệu này ghi lại kết quả kiểm chứng tự động trên môi trường phát triển Windows và danh mục kiểm thử thực tế trên iPhone thật cho kiến trúc **ChatLink V2**.

---

## 1. Kết quả kiểm chứng tự động trên Windows

| Hạng mục kiểm tra | Lệnh thực thi | Kết quả quan sát | Ghi chú |
| :--- | :--- | :--- | :--- |
| **TypeScript Typecheck** | `npm.cmd run check` | **PASS (0 errors)** | Kiểm tra kiểu chặt chẽ cho toàn bộ `@chatlink/protocol`, `@chatlink/mobile-web` và `@chatlink/desktop`. |
| **Kiểm thử Rust Backend** | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | **PASS (8/8 tests passed)** | Đầy đủ 8 bài kiểm thử: bộ phân tích và tạo gói tin UDP mDNS, nhận diện hostname, ưu tiên card mạng LAN vật lý, lưu trữ SQLite migration, khử trùng lặp tin nhắn, và quy trình ghép đôi/hủy ghép đôi. |
| **Kiểm thử Mobile Web & IndexedDB** | `npm.cmd run test:storage` | **PASS** | Kiểm tra lưu trữ tin nhắn ngoại tuyến IndexedDB, cơ chế gửi bù Outbox khi có mạng, phân trang tin nhắn và Service Worker offline shell. |
| **Đóng gói Bundle Release** | `tauri build` / `cargo build --release` | **PASS** | Biên dịch thành công binary độc lập [`chatlink-desktop.exe`](file:///d:/Desktop/NHAP/Personal_Project/ChatLink/apps/desktop/src-tauri/target/release/chatlink-desktop.exe) kích thước 15.1 MB. |

### Chi tiết các Unit Tests Backend Rust:
```text
running 8 tests
test mdns::tests::test_contains_query ... ok
test mdns::tests::test_build_a_response ... ok
test network::tests::hostname_is_not_empty ... ok
test network::tests::physical_interfaces_are_preferred ... ok
test server::tests::test_create_core_snapshot ... ok
test db::tests::pairing_and_unpair_preserves_messages ... ok
test db::tests::duplicate_insert_keeps_one_message_and_cursor_order ... ok
test db::tests::migration_preserves_milestone_three_history ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

---

## 2. Kế hoạch nghiệm thu trên thiết bị thật (iPhone + PC Windows)

Sau khi khởi chạy ứng dụng desktop, thực hiện nghiệm thu theo kịch bản người dùng thực tế:

### Kịch bản 1: Thiết lập ghép đôi lần đầu (1-Touch Pairing)
1. Kết nối iPhone và PC Windows vào cùng một mạng Wi-Fi.
2. Mở `chatlink-desktop.exe` trên Windows.
3. Trên iPhone, mở trình duyệt Safari và truy cập `http://chatlink.local:42100` (hoặc quét mã QR trên màn hình Desktop).
4. Quan sát: Trang web mở lên ngay lập tức, không có bất kỳ cảnh báo chứng chỉ SSL hay yêu cầu cài đặt file Profile nào.
5. Trên iPhone, bấm nút **"Yêu cầu kết nối"**.
6. Trên Desktop, xuất hiện banner thông báo: *"Thiết bị [Tên iPhone] yêu cầu ghép đôi"*. Bấm nút **[Cho phép kết nối]**.
7. Quan sát: Cả 2 màn hình lập tức chuyển vào giao diện chat, trạng thái chuyển sang xanh lá **"Đã kết nối"**.

### Kịch bản 2: Cài đặt WebClip Home Screen
1. Trên Safari của iPhone khi đang ở trang chat, bấm nút **Chia sẻ (Share)** $\rightarrow$ chọn **"Thêm vào Màn hình chính" (Add to Home Screen)**.
2. Đặt tên "ChatLink" và bấm Thêm.
3. Thoát Safari, mở ứng dụng từ biểu tượng ChatLink trên màn hình chính.
4. Quan sát: Ứng dụng chạy full màn hình (không có thanh URL hay thanh điều hướng Safari), tự động kết nối và tải lại toàn bộ lịch sử tin nhắn.

### Kịch bản 3: Tự động kết nối lại khi mở lại app (Zero-touch Auto-Connect)
1. Đóng app ChatLink trên iPhone hoặc tắt màn hình trong 30 giây.
2. Mở lại app ChatLink trên iPhone.
3. Quan sát: Hệ thống tự động kích hoạt `fast reconnect` ở thời điểm $t = 0\text{ ms}$, kết nối lại thành công sau `< 200\text{ ms}` mà không cần thao tác nào từ người dùng.

### Kịch bản 4: Đổi IP mạng Wi-Fi (DHCP IP Change)
1. Ngắt kết nối Wi-Fi trên máy tính và kết nối lại (hoặc khởi động lại router để nhận IP LAN mới).
2. Mở app ChatLink trên iPhone.
3. Quan sát: Nhờ tên miền mDNS `chatlink.local`, iPhone vẫn tìm thấy máy tính và kết nối thành công, không bị lỗi mất Origin hay mất dữ liệu IndexedDB như phiên bản V1.

# Kiểm chứng ChatLink V1 — 24/09/2026

## Đã chạy trên Windows

| Hạng mục | Kết quả quan sát được |
| --- | --- |
| `npm.cmd run check` | PASS: TypeScript cho protocol, desktop và mobile. |
| `npm.cmd run test:storage` | PASS: IndexedDB sau reload, ACK, sync, nhập outbox; Chrome headless ở 320 px và 430 px không tràn ngang; app shell production mở lại được khi giả lập offline. |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | PASS: 10 kiểm thử Rust, gồm migration từ M3, ghi trùng, phân trang, pairing/token, Origin, ACK sau SQLite, giới hạn 8 KB và 30 tin/10 giây, delivered đúng tin Windows, unpair giữ lịch sử, SAN IP và CA hỏng. |
| `npm.cmd run build` | PASS: tạo `target/release/chatlink-desktop.exe` và `target/release/bundle/nsis/ChatLink_0.1.0_x64-setup.exe`. |
| Bản cài NSIS | PASS: cài thử vào thư mục tạm trong workspace, đối chiếu SHA-256 `mobile-web/index.html` với bản build, chạy ứng dụng đã cài và nhận HTTP 200 tại `/health`, `/`, `/sw.js` trên `192.168.0.100:42100`; sau đó gỡ bản cài thử. |
| Profile CA | PASS: endpoint `42101/chatlink-root.mobileconfig` trả profile chứa đúng bytes của chứng chỉ CA công khai trên Windows; không chứa khóa riêng. |

Kiểm tra ứng dụng và DPAPI được chạy bằng tài khoản Windows sở hữu CA. Tài khoản sandbox khác không giải mã được khóa DPAPI; đó là giới hạn theo tài khoản Windows, đã ghi trong README.

## Chưa chạy trên iPhone thật

Các mục sau là **NOT RUN** trong môi trường này: cài và bật tin cậy CA trên iOS, Safari và Home Screen, kết nối cùng Wi-Fi qua WSS, chat hai chiều trên thiết bị, mở lại hai ứng dụng, mất và phục hồi Wi-Fi thật, đổi IP, thử thiết bị chưa ghép đôi, và đo độ trễ LAN. Mục tiêu `<200 ms` chưa được xác nhận. Chỉ coi bản V1 đủ điều kiện phát hành sau khi các bước này được quan sát và ghi kết quả trên iPhone thật.

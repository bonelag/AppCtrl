# AppCtrl ⚡

AppCtrl là một trình quản lý ứng dụng hiện đại, giao diện đẹp mắt dành cho Windows, được xây dựng bằng **Tauri v2**, **Rust** và **SolidJS**.

Ứng dụng giúp bạn quản lý, khởi chạy và theo dõi trạng thái của các file thực thi (EXE), script (BAT, Shell) một cách dễ dàng, tập trung, đồng thời tích hợp các công cụ hệ thống mạnh mẽ phục vụ cho nhà phát triển và người dùng chuyên sâu.

## ✨ Tính năng nổi bật

*   **🚀 Quản lý & Khởi chạy tập trung**: Thêm và quản lý các ứng dụng EXE, BAT, Shell script trong một giao diện duy nhất.
*   **💾 Portable hoàn toàn**: Cấu hình và dữ liệu được lưu vào file `config.json` ngay cạnh file chạy, dễ dàng sao chép và di chuyển đi mọi nơi.
*   **🎨 Giao diện Premium**: Thiết kế Modern Glassmorphism, hiệu ứng micro-interactions mượt mà, hỗ trợ cả hai chế độ sáng/tối (Light/Dark Mode).
*   **🛡️ Giám sát trạng thái**: Tự động phát hiện ứng dụng đang chạy (dựa trên tên Process) và cập nhật trạng thái Real-time.
*   **🖼️ Trích xuất Icon sắc nét**: Tự động trích xuất icon độ phân giải cao (Jumbo 256x256) từ file thực thi EXE.
*   **📜 Log tương tác**: Xem log output của ứng dụng thời gian thực, hỗ trợ copy nhanh và nhấp chuột mở link trực tiếp.
*   **🗡️ Task Killer**: Trình quản lý tác vụ mạnh mẽ. Xem danh sách tiến trình, gom nhóm theo tên, hiển thị dung lượng RAM sử dụng và tắt nhanh các ứng dụng bị treo.
*   **🔪 Port Killer**: Xem nhanh các cổng mạng (port TCP/UDP) đang mở, xác định tiến trình (process) nào đang chiếm dụng port và tắt chúng chỉ với 1 click.
*   **📁 Mini File Explorer**: Trình duyệt file tích hợp tiện dụng với các tính năng cao cấp:
    *   *Trực quan hóa ổ đĩa*: Hiển thị các ổ đĩa trên máy tính kèm thông tin dung lượng trống, tổng dung lượng và thanh tiến trình màu sắc trực quan.
    *   *Hiển thị Icon hệ thống*: Tự động trích xuất và hiển thị icon hệ thống chính xác cho từng loại tệp tin và thư mục.
    *   *Bố cục tối ưu*: Giao diện tự động co giãn vừa vặn theo chiều ngang, tự động rút gọn tên file/thư mục quá dài thành định dạng dấu ba chấm (`abc...`).
    *   *Thao tác tệp tiện lợi*: Menu chuột phải hỗ trợ Mở bằng Explorer hệ thống (trạng thái `/select` chọn đúng file), Sao chép (Copy), Dán (Paste) đệ quy, Xóa (Delete) thường.
    *   *Tiến trình chiếm dụng (Process Used)*: Sử dụng Windows Restart Manager API để kiểm tra chính xác tiến trình nào đang khóa file/thư mục khiến bạn không thể chỉnh sửa hoặc xóa, đi kèm nút **Kill** để tắt nóng từng tiến trình.
    *   *Xóa cưỡng ép (Force Delete)*: Tự động tắt các tiến trình đang chiếm dụng file/thư mục, sau đó di chuyển chúng vào Thùng rác (Recycle Bin) của Windows bằng `SHFileOperationW` (có thể khôi phục lại khi cần thiết).
    *   *Quét dung lượng thư mục thủ công (Click-to-Scan)*: Mặc định chỉ hiển thị dung lượng của file đơn lẻ để tối ưu hiệu năng đĩa. Đối với thư mục, nút **Scan** nhỏ gọn sẽ hiển thị; khi click vào, hệ thống sẽ thực hiện quét đệ quy toàn bộ thư mục con (không giới hạn số lượng file) để đưa ra dung lượng chuẩn xác nhất.

## 🛠️ Công nghệ sử dụng

*   **Core**: [Tauri v2](https://v2.tauri.app/) (Rust) - Siêu nhẹ, bảo mật và hiệu năng cao.
*   **Frontend**: [SolidJS](https://www.solidjs.com/) - Hiệu năng render vượt trội, phản hồi tức thì.
*   **Styling**: Vanilla CSS kết hợp TailwindCSS - Giao diện hiện đại, dễ dàng tùy biến.
*   **Build**: Vite.

## 🚀 Cài đặt và Chạy thử

### Yêu cầu hệ thống
*   Node.js (v18+)
*   Rust & Cargo (phiên bản mới nhất)
*   Visual Studio C++ Build Tools (dành cho Windows)

### Phát triển (Dev)

1.  Clone dự án:
    ```bash
    git clone https://github.com/your-username/AppCtrl.git
    cd AppCtrl
    ```

2.  Cài đặt dependencies:
    ```bash
    npm install
    ```

3.  Chạy chế độ development:
    ```bash
    npm run tauri dev
    ```

### Đóng gói (Build)

Để tạo file `AppCtrl.exe` chạy ngay (Portable):

```bash
npm run tauri build
```

File kết quả sau khi đóng gói sẽ nằm tại: `src-tauri/target/release/AppCtrl.exe`

## 📂 Cấu trúc dự án

*   `src/`: Mã nguồn Frontend (SolidJS, components, giao diện File Explorer, Task Killer, Port Killer...).
*   `src-tauri/`: Mã nguồn Backend (Rust).
*   `src-tauri/src/lib.rs`: Logic chính của Backend (Tương tác API Windows, lấy thông tin tiến trình khóa, quản lý khay hệ thống, trích xuất icon...).

## 📝 License

Dự án được phân phối dưới giấy phép MIT License.

# AppCtrl ⚡

AppCtrl là một trình quản lý ứng dụng hiện đại, giao diện đẹp mắt dành cho Windows, được xây dựng bằng **Tauri v2**, **Rust** và **SolidJS**.

Ứng dụng giúp bạn quản lý, khởi chạy và theo dõi trạng thái của các file thực thi (EXE), script (BAT, Shell) một cách dễ dàng và tập trung.

## ✨ Tính năng nổi bật

*   **Quản lý tập trung**: Thêm và quản lý các ứng dụng EXE, BAT, Shell script trong một giao diện duy nhất.
*   **Portable hoàn toàn**: Cấu hình và dữ liệu được lưu vào file `config.json` ngay cạnh file chạy, dễ dàng sao chép và di chuyển.
*   **Giao diện hiện đại**: Thiết kế Glassmorphism, hiệu ứng mượt mà, hỗ trợ Dark Mode.
*   **Minimize to Tray**: Thu nhỏ xuống khay hệ thống để chạy ngầm, không chiếm chỗ trên Taskbar.
*   **Giám sát trạng thái**: Tự động phát hiện ứng dụng đang chạy (dựa trên tên Process) và cập nhật trạng thái Real-time.
*   **Icon sắc nét**: Tự động trích xuất icon độ phân giải cao (Jumbo 256x256) từ file EXE.
*   **Log tương tác**: Xem log output của ứng dụng, hỗ trợ copy và click link trực tiếp.

## 🛠️ Công nghệ sử dụng

*   **Core**: [Tauri v2](https://v2.tauri.app/) (Rust) - Siêu nhẹ, bảo mật và hiệu năng cao.
*   **Frontend**: [SolidJS](https://www.solidjs.com/) - Hiệu năng render vượt trội.
*   **Styling**: [TailwindCSS](https://tailwindcss.com/) - Thiết kế nhanh chóng và linh hoạt.
*   **Build**: Vite.

## 🚀 Cài đặt và Chạy thử

### Yêu cầu
*   Node.js (v16+)
*   Rust (mới nhất)
*   Visual Studio C++ Build Tools (cho Windows)

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

File kết quả sẽ nằm tại: `src-tauri/target/release/appctrl.exe`

## 📂 Cấu trúc dự án

*   `src/`: Mã nguồn Frontend (SolidJS, Tailwind).
*   `src-tauri/`: Mã nguồn Backend (Rust).
*   `src-tauri/src/lib.rs`: Logic chính của Backend (Quản lý process, icon, tray...).

## 📝 License

MIT License.

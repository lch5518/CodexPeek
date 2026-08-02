# CodexPeek – Codex Usage Monitor for Windows

**Languages:** [English (default)](../../README.md) · [한국어](README.ko.md) · [Español](README.es.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [日本語](README.ja.md) · [हिन्दी](README.hi.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [Tiếng Việt](README.vi.md) · [Türkçe](README.tr.md) · [العربية](README.ar.md)

Codex Usage Monitor là một widget Windows gốc nhỏ giúp bạn xem nhanh mức sử dụng Codex.
Ứng dụng hiển thị các cửa sổ giới hạn tốc độ chính và phụ trên thanh tác vụ, trong widget nổi và trong khay hệ thống.

![Widget Codex Usage Monitor trên thanh tác vụ](../images/taskbar-widget-en.png)

## Điểm nổi bật

- Hiển thị các cửa sổ mức sử dụng Codex chính và phụ, bao gồm thời điểm đặt lại.
- Ước tính thời điểm mỗi cửa sổ có thể cạn dựa trên các lần quan sát thành công gần đây và hiển thị
  ước tính trong chi tiết sử dụng cũng như tooltip thanh tác vụ (tính năng mới của bản phát hành).
- Dùng giao diện `app-server` của Codex CLI đã cài đặt thay vì phân tích các tệp xác thực.
- Cho phép chọn thủ công trong tối đa tám hồ sơ sử dụng được cô lập.
- Hỗ trợ hiển thị widget trên mọi thanh tác vụ hoặc chỉ trên màn hình chính.
- Tự động chuyển an toàn sang widget nổi và biểu tượng khay khi không thể gắn vào thanh tác vụ.
- Hỗ trợ làm mới thủ công, khoảng thời gian làm mới tự động, khởi động cùng Windows, chẩn đoán và giao diện đã bản địa hóa.

## Cách hoạt động

Trình giám sát khởi chạy `codex app-server --stdio` dưới dạng tiến trình con cục bộ và trao đổi thông điệp JSONL qua đầu vào và đầu ra chuẩn.
Codex CLI đã cài đặt tự xử lý xác thực của nó và có thể liên hệ với OpenAI theo cấu hình và chính sách mạng hiện có.

Trình giám sát chỉ yêu cầu trạng thái đăng nhập và các cửa sổ mức sử dụng cần thiết để hiển thị.
Ứng dụng không bắt đầu tác vụ Codex và không gọi `codex exec`.

## Hồ sơ sử dụng

Hồ sơ hệ thống **Tài khoản Codex mặc định** không thể xóa dùng thư mục Codex được kế thừa khi
CodexPeek khởi động, hoặc giá trị mặc định của CLI nếu chưa đặt `CODEX_HOME`. Mỗi hồ sơ
được quản lý dùng một thư mục Codex riêng bên dưới
`%APPDATA%\CodexPeek\profiles`. Tổng giới hạn là tám hồ sơ, bao gồm hồ sơ hệ thống.

Nhãn hồ sơ do bạn tự đặt. CodexPeek không kiểm tra email hoặc ID tài khoản, vì vậy hãy xác
nhận tài khoản ChatGPT dự định dùng trong trình duyệt khi thêm hồ sơ hoặc đăng nhập lại.
Việc chọn hồ sơ chỉ thay đổi mức sử dụng mà CodexPeek truy vấn và hiển thị. Đăng nhập trong
terminal, IDE, ứng dụng Codex, WSL, Remote SSH và Dev Containers không thay đổi.

Việc chọn luôn là thủ công. CodexPeek không tự động chọn hoặc luân phiên hồ sơ theo hạn mức
còn lại và không định tuyến công việc Codex qua hồ sơ. Xóa hồ sơ được quản lý sẽ xóa vĩnh
viễn dữ liệu cục bộ, bao gồm thông tin xác thực CLI được lưu riêng; hãy đọc kỹ xác nhận.

CodexPeek không bao giờ đọc, phân tích cú pháp hoặc sao chép `auth.json` của bất kỳ hồ sơ
nào. Chỉ tiến trình con `app-server` của hồ sơ được quản lý nhận `CODEX_HOME` và thiết lập
kho thông tin xác thực dạng tệp của hồ sơ đó. Chẩn đoán chỉ ghi số lượng tổng hợp, không
ghi nhãn, đường dẫn hay dữ liệu tài khoản.

### Trình quản lý hồ sơ

Bạn có thể đổi tên hồ sơ hệ thống, nhưng không thể đăng xuất hoặc xóa hồ sơ đó. Tên tùy chỉnh
của hồ sơ hệ thống chỉ thay đổi nội dung CodexPeek hiển thị; đó không phải là danh tính tài
khoản. Chỉ trình quản lý hồ sơ đánh dấu hồ sơ này là tài khoản mặc định.

Trình đơn con **Hồ sơ sử dụng** trong khay cho phép chọn hồ sơ và mở **Quản lý hồ sơ sử dụng**;
không có lệnh thêm ở đó. Chỉ thêm hồ sơ bằng `+` bên dưới danh sách trong trình quản lý. Không
có nút Đóng hoặc Thêm ở cuối cửa sổ: dùng `X` của cửa sổ hoặc Esc để đóng trình quản lý.

## Yêu cầu

- Windows 10 hoặc Windows 11, x64.
- [Codex CLI](https://github.com/openai/codex) đã đăng nhập và hỗ trợ `account/read` cùng `account/rateLimits/read`.

## Tải xuống và chạy

Trước tiên hãy xác minh rằng Codex CLI đã được cài đặt và đã đăng nhập:

```powershell
codex --version
codex login status
```

### Trình cài đặt (khuyến nghị)

1. Tải `CodexPeek-Setup-v<version>-x64.exe` từ
   [GitHub Release mới nhất](https://github.com/lch5518/CodexPeek/releases/latest).
2. Chạy trình cài đặt và làm theo các lời nhắc. Không cần quyền quản trị viên.
3. Mở **Codex Usage Monitor** từ Start Menu.

### Bản portable

1. Tải `codex-peek-v<version>-windows-x86_64-portable.zip` từ
   release mới nhất.
2. Giải nén toàn bộ ZIP vào một thư mục có quyền ghi.
3. Chạy `codex-peek.exe` từ thư mục đã giải nén.

### Build từ mã nguồn

Tùy chọn này yêu cầu Rust 1.85 trở lên, Visual Studio 2022 C++ Build Tools và
Windows SDK. Nó chạy ứng dụng từ repository đã clone và không tạo lối tắt Start
Menu hoặc trình gỡ cài đặt.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

Để kiểm tra bản build và kết nối Codex CLI mà không mở giao diện:

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Yêu cầu Codex cài đặt

Sao chép prompt bên dưới vào Codex. Prompt này ưu tiên Trình cài đặt đã xác minh và chỉ chuyển sang
build từ mã nguồn khi không có artefact Release tương thích.

```text
Cài đặt CodexPeek trên máy tính Windows x64 này và hoàn tất phần xác minh giúp tôi.

1. Xác nhận đây là Windows x64, rồi chạy `codex --version` và `codex login status`.
2. Chỉ dùng repository chính thức và các Releases của repository đó:
   https://github.com/lch5518/CodexPeek
3. Ưu tiên `CodexPeek-Setup-v<version>-x64.exe` mới nhất. Tải nó cùng với
   `SHA256SUMS.txt`, tìm đúng mục Installer trong tệp đó, tính SHA-256 của
   Installer và chỉ tiếp tục nếu các hash khớp nhau. Không tắt các biện pháp
   bảo mật và không chạy tệp có checksum bị thiếu hoặc khác.
4. Cài đặt cho người dùng hiện tại mà không yêu cầu quyền quản trị viên. Giữ nguyên
   các cài đặt CodexPeek hiện có và không dừng ứng dụng đang chạy hoặc tiến trình
   không liên quan; hãy cho tôi biết nếu tôi cần tự đóng ứng dụng.
5. Chỉ khi không có artefact Release tương thích, clone repository chính thức
   vào một thư mục mới mà người dùng có quyền ghi và chạy `cargo build --release`.
   Nếu cần cài Git, Rust 1.85+, Visual Studio 2022 C++ Build Tools hoặc Windows SDK,
   trước tiên hãy giải thích chính xác điều gì sẽ thay đổi và xin tôi phê duyệt.
6. Không bao giờ đọc hoặc in nội dung của `%USERPROFILE%\.codex\auth.json`. Việc xác thực
   chỉ được xử lý thông qua Codex CLI đã cài đặt.
7. Sau khi cài đặt hoặc build, chạy `codex-peek.exe --diagnose` thu được. Nếu lệnh
   thành công, hãy khởi chạy CodexPeek.
8. Báo cáo phương thức cài đặt đã chọn, phiên bản đã cài, vị trí tệp thực thi,
   kết quả checksum và kết quả chẩn đoán. Nếu có lỗi, hãy dừng an toàn và giải thích
   đúng điểm bị chặn mà không để lộ thông tin nhạy cảm.
```

Các bản Installer và Portable dùng `%APPDATA%\CodexPeek\settings.json`, nên
cài đặt sẽ được chia sẻ nếu bạn chuyển đổi giữa hai bản. Trình cài đặt thêm lối tắt Start Menu
nhưng không bật khởi động cùng Windows theo mặc định.

Các release ban đầu chưa được ký mã và có thể kích hoạt Microsoft Defender SmartScreen.
Chỉ tải xuống từ release chính thức và xác minh tệp với `SHA256SUMS.txt`.

Xem [hướng dẫn cài đặt chi tiết (tiếng Hàn)](../INSTALL.md) để biết cách xác minh hash,
cập nhật, hành vi gỡ cài đặt, chẩn đoán và khắc phục sự cố.

## Sử dụng trình giám sát

Dùng menu khay để làm mới mức sử dụng, chọn khoảng thời gian làm mới 1/5/10/15/30 phút, và hiển thị hoặc ẩn widget.
Menu này cũng cung cấp cài đặt khởi động cùng Windows, chế độ xem khi khởi động, làm mới xác thực, tự động làm mới xác thực, ngôn ngữ và chẩn đoán.
Chọn **Widget: all monitors** hoặc **Widget: primary monitor only** để kiểm soát vị trí trên nhiều màn hình; lựa chọn này được ghi nhớ qua các lần khởi động lại.

Theo mặc định, ngôn ngữ giao diện đi theo locale của Windows khi locale đó khớp với một ngôn ngữ được hỗ trợ. Bạn cũng có thể chọn ngôn ngữ thủ công từ menu khay. Các ngôn ngữ được hỗ trợ gồm tiếng Hàn, tiếng Anh, tiếng Tây Ban Nha, tiếng Bồ Đào Nha Brazil, tiếng Indonesia, tiếng Nhật, tiếng Hindi, tiếng Đức, tiếng Pháp, tiếng Việt, tiếng Thổ Nhĩ Kỳ và tiếng Ả Rập.

Widget trên thanh tác vụ dùng theme sáng/tối của hệ thống Windows cho phần chữ và để vật liệu gốc của thanh tác vụ hiện qua nền.

Mỗi lần chỉ có một yêu cầu mức sử dụng được chạy. Các yêu cầu thất bại sẽ được thử lại với độ trễ tăng dần trong khi các giá trị thành công gần nhất vẫn hiển thị.

Nếu widget thanh tác vụ không thể gắn lại sau khi Explorer khởi động lại hoặc bố cục thanh tác vụ thay đổi, biểu tượng khay vẫn khả dụng và trình giám sát sẽ thử lại an toàn.

Khi bật dự báo (mặc định), chỉ các lần quan sát thành công được lưu trong tệp cục bộ riêng
`%APPDATA%\CodexPeek\usage-history.json`. Dự báo chỉ xuất hiện khi có đủ dữ liệu mới từ cùng hồ sơ,
cửa sổ và chu kỳ đặt lại; dữ liệu mới hoặc cũ được ghi rõ là đang thu thập hoặc đã cũ thay vì hiển
thị như dự báo hiện tại. Trong menu khay **Usage forecasting**, bạn có thể tắt tính năng hoặc chọn
**Clear usage forecast history**; xóa hồ sơ được quản lý cũng xóa lịch sử của hồ sơ đó. Đây là ước
tính cục bộ, không đảm bảo chính sách giới hạn của OpenAI và không bao giờ được tải lên hay đồng bộ.

## Quyền riêng tư và bảo mật

Trình giám sát không bao giờ đọc hoặc phân tích nội dung của `%USERPROFILE%\.codex\auth.json`.
Chẩn đoán chỉ kiểm tra xem đường dẫn đó có tồn tại hay không.

Phản hồi RPC thô chỉ được xử lý đủ lâu để trích xuất loại đăng nhập và các trường giới hạn tốc độ được hiển thị.
Token, ID tài khoản, địa chỉ email, nội dung tệp xác thực và giá trị proxy không được lưu trữ hoặc ghi vào log.

Cài đặt được lưu trong `%APPDATA%\CodexPeek\settings.json`.
Log chẩn đoán có giới hạn được lưu trong `%TEMP%\codex-peek.log`.

`usage-history.json` chỉ chứa ID hồ sơ nội bộ, `Primary` hoặc `Secondary`, phần trăm sử dụng, thời
điểm đặt lại tùy chọn và thời điểm quan sát thành công. Tệp không chứa email, ID tài khoản, tên hoặc
thư mục gốc hồ sơ, token, nội dung tệp xác thực, cuộc trò chuyện/prompt, cài đặt proxy hay phản hồi
RPC thô. Dữ liệu được giữ tối đa 30 ngày và 1.000 mẫu cho mỗi hồ sơ/cửa sổ; các giá trị lặp lại và
lần quan sát cách nhau dưới năm phút được bỏ qua để giảm ghi đĩa. Tệp hỏng được cách ly hoặc đặt lại
mà không ngăn hiển thị mức sử dụng.

Sau khi xác nhận, **Clear usage forecast history** xóa toàn bộ mẫu. Installer và Portable giữ lại
`%APPDATA%\CodexPeek` khi gỡ cài đặt, vì vậy lịch sử có thể còn sau khi xóa ứng dụng; hãy dùng thao tác
trong khay hoặc xóa tệp/thư mục thủ công để dọn dẹp hoàn toàn.

Để xem hướng dẫn đầy đủ về xử lý dữ liệu và báo cáo lỗ hổng, hãy xem [SECURITY.md](../../SECURITY.md).

## Khắc phục sự cố

| Vấn đề | Cách xử lý |
| --- | --- |
| Không tìm thấy Codex CLI | Chạy `codex --version` và `where.exe codex`, rồi bảo đảm Codex CLI có trong `PATH`. |
| CLI không được hỗ trợ | Cập nhật Codex CLI. Hỗ trợ RPC bắt buộc quan trọng hơn số phiên bản được hiển thị. |
| Đã đăng xuất hoặc xác thực hết hạn | Hoàn tất luồng đăng nhập thông thường trong Codex CLI, rồi chọn **Refresh authentication** trong menu khay. |
| Widget thanh tác vụ nằm trên sai màn hình | Chọn **Widget: all monitors** hoặc **Widget: primary monitor only** từ menu khay. |
| Thiếu widget thanh tác vụ | Dùng widget nổi hoặc biểu tượng khay, khởi động lại Explorer nếu cần, rồi chọn chế độ màn hình widget mong muốn. |
| Cần thêm chi tiết | Chạy `--diagnose` hoặc mở **Diagnostics** từ menu khay. |

## Phát triển

Build từ mã nguồn yêu cầu Rust 1.85 trở lên, Visual Studio 2022 C++ Build Tools và
Windows SDK. Build và xác thực từ thư mục gốc của repository:

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Các kiểm tra tự động không thay thế các kịch bản Windows, DPI, nhiều màn hình và phục hồi Explorer trong [release checklist](../RELEASE_CHECKLIST.md).

## ❤️ Hỗ trợ

Nếu CodexPeek giúp bạn tiết kiệm thời gian, hãy cân nhắc hỗ trợ quá trình phát triển.

- ⭐ Star repository này
- ❤️ [Tài trợ trên GitHub](https://github.com/sponsors/lch5518)

Mỗi lượt tài trợ giúp dự án tiếp tục được duy trì tích cực.

## Giấy phép

Dự án này được cung cấp theo [Giấy phép MIT](../../LICENSE).
Xem [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) để biết các thông báo của bên thứ ba.

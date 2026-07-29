# CodexPeek – Codex Usage Monitor for Windows

**Languages:** [English (default)](../../README.md) · [한국어](README.ko.md) · [Español](README.es.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [日本語](README.ja.md) · [हिन्दी](README.hi.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [Tiếng Việt](README.vi.md) · [Türkçe](README.tr.md) · [العربية](README.ar.md)

Codex Usage Monitor adalah widget native Windows kecil untuk memeriksa penggunaan Codex dengan cepat.
Aplikasi ini menampilkan jendela batas penggunaan utama dan sekunder di taskbar, widget mengambang, dan system tray.

![Widget taskbar Codex Usage Monitor](../images/taskbar-widget-en.png)

## Sorotan

- Menampilkan jendela penggunaan Codex utama dan sekunder, termasuk waktu reset.
- Menggunakan antarmuka `app-server` dari Codex CLI yang terpasang, bukan mem-parsing file autentikasi.
- Memungkinkan Anda memilih secara manual dari maksimal delapan profil penggunaan yang terisolasi.
- Mendukung tampilan widget di setiap taskbar atau hanya di monitor utama.
- Beralih dengan aman ke widget mengambang dan ikon tray ketika penempelan ke taskbar tidak tersedia.
- Mendukung refresh manual, interval refresh otomatis, startup Windows, diagnostik, dan UI terlokalisasi.

## Cara kerjanya

Monitor menjalankan `codex app-server --stdio` sebagai proses anak lokal dan bertukar pesan JSONL melalui input dan output standar.
Codex CLI yang terpasang menangani autentikasinya sendiri dan dapat menghubungi OpenAI sesuai konfigurasi dan kebijakan jaringan yang sudah ada.

Monitor hanya meminta status masuk dan jendela penggunaan yang diperlukan untuk tampilan.
Aplikasi ini tidak memulai tugas Codex atau memanggil `codex exec`.

## Profil penggunaan

Profil sistem **Akun Codex default** yang tidak dapat dihapus memakai Codex home yang diwarisi
saat CodexPeek dimulai, atau nilai bawaan CLI jika `CODEX_HOME` tidak ditetapkan. Setiap
profil terkelola memakai Codex home terpisah di bawah
`%APPDATA%\CodexPeek\profiles`. Batasnya delapan profil secara keseluruhan,
termasuk profil sistem.

Label profil Anda tentukan sendiri. CodexPeek tidak memeriksa email atau ID akun, jadi
konfirmasikan akun ChatGPT yang dimaksud di browser saat menambah profil atau masuk lagi.
Pemilihan hanya mengubah penggunaan yang diambil dan ditampilkan CodexPeek. Login di
terminal, IDE, aplikasi Codex, WSL, Remote SSH, dan Dev Containers tidak berubah.

Pemilihan selalu manual. CodexPeek tidak memilih atau merotasi profil secara otomatis
berdasarkan sisa batas dan tidak merutekan pekerjaan Codex melalui profil. Menghapus
profil terkelola akan menghapus permanen data lokalnya, termasuk kredensial CLI yang
disimpan terpisah; periksa konfirmasi dengan cermat.

CodexPeek tidak pernah membaca, mem-parsing, atau menyalin `auth.json` profil mana pun.
Hanya proses anak `app-server` untuk profil terkelola yang menerima `CODEX_HOME` dan
pengaturan penyimpanan kredensial file miliknya. Diagnostik hanya mencatat jumlah agregat,
tanpa label, jalur, atau data akun.

### Pengelola profil

Anda dapat mengganti nama profil sistem, tetapi tidak dapat keluar atau menghapusnya. Nama
khusus profil sistem hanya mengubah tampilan CodexPeek; nama tersebut bukan identitas akun.
Hanya pengelola profil yang menandainya sebagai akun default.

Submenu baki **Profil penggunaan** memungkinkan Anda memilih profil dan membuka **Kelola
profil penggunaan**; tidak ada perintah tambah di sana. Tambahkan profil hanya dengan `+` di
bawah daftar pengelola. Tidak ada tombol Tutup atau Tambah di bagian bawah: gunakan `X` jendela
atau Esc untuk menutup pengelola.

## Persyaratan

- Windows 10 atau Windows 11, x64.
- [Codex CLI](https://github.com/openai/codex) yang sudah masuk dan mendukung `account/read` serta `account/rateLimits/read`.

## Unduh dan jalankan

Pertama, pastikan Codex CLI sudah terpasang dan sudah masuk:

```powershell
codex --version
codex login status
```

### Installer (direkomendasikan)

1. Unduh `CodexPeek-Setup-v<version>-x64.exe` dari
   [GitHub Release terbaru](https://github.com/lch5518/CodexPeek/releases/latest).
2. Jalankan setup dan ikuti instruksinya. Akses administrator tidak diperlukan.
3. Buka **Codex Usage Monitor** dari Start Menu.

### Portable

1. Unduh `codex-peek-v<version>-windows-x86_64-portable.zip` dari release terbaru.
2. Ekstrak ZIP sepenuhnya ke folder yang dapat ditulis.
3. Jalankan `codex-peek.exe` dari folder hasil ekstraksi.

### Build dari sumber

Opsi ini memerlukan Rust 1.85 atau lebih baru, Visual Studio 2022 C++ Build Tools, dan
Windows SDK. Opsi ini menjalankan aplikasi dari repositori yang dikloning dan tidak membuat
shortcut Start Menu atau uninstaller.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

Untuk memeriksa build dan koneksi Codex CLI tanpa membuka UI:

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Minta Codex menginstalnya

Salin prompt di bawah ini ke Codex. Prompt ini mengutamakan Installer terverifikasi dan hanya beralih ke
build dari sumber jika aset Release yang kompatibel tidak tersedia.

```text
Instal CodexPeek di komputer Windows x64 ini dan selesaikan verifikasinya untuk saya.

1. Pastikan komputer ini adalah Windows x64, lalu jalankan `codex --version` dan `codex login status`.
2. Gunakan hanya repositori resmi dan Releases-nya:
   https://github.com/lch5518/CodexPeek
3. Utamakan `CodexPeek-Setup-v<version>-x64.exe` terbaru. Unduh bersama
   `SHA256SUMS.txt`, temukan entri Installer yang tepat di file tersebut, hitung
   SHA-256 Installer, dan lanjutkan hanya jika hash cocok. Jangan menonaktifkan
   kontrol keamanan atau menjalankan file yang checksumnya hilang atau berbeda.
4. Instal untuk pengguna saat ini tanpa meminta akses administrator. Pertahankan
   pengaturan CodexPeek yang sudah ada dan jangan hentikan aplikasi yang sedang berjalan
   atau proses yang tidak terkait; beri tahu saya jika saya perlu menutup aplikasinya sendiri.
5. Hanya jika aset Release yang kompatibel tidak tersedia, clone repositori resmi
   ke direktori baru yang dapat ditulis pengguna dan jalankan `cargo build --release`.
   Jika Git, Rust 1.85+, Visual Studio 2022 C++ Build Tools, atau Windows SDK harus
   diinstal, jelaskan terlebih dahulu secara tepat apa yang akan berubah dan minta
   persetujuan saya.
6. Jangan pernah membaca atau mencetak isi `%USERPROFILE%\.codex\auth.json`. Autentikasi
   harus ditangani hanya melalui Codex CLI yang terinstal.
7. Setelah instalasi atau build, jalankan `codex-peek.exe --diagnose` yang dihasilkan.
   Jika berhasil, luncurkan CodexPeek.
8. Laporkan metode instalasi yang dipilih, versi yang terinstal, lokasi executable,
   hasil checksum, dan hasil diagnostik. Jika ada yang gagal, berhenti dengan aman dan
   jelaskan blocker tepatnya tanpa mengekspos informasi sensitif.
```

Edisi Installer dan Portable menggunakan `%APPDATA%\CodexPeek\settings.json`, sehingga
pengaturan dibagikan jika Anda beralih di antara keduanya. Installer menambahkan shortcut Start Menu,
tetapi tidak mengaktifkan startup Windows secara default.

Rilis awal tidak ditandatangani kode dan dapat memicu Microsoft Defender SmartScreen.
Unduh hanya dari release resmi dan verifikasi file terhadap `SHA256SUMS.txt`.

Lihat [panduan instalasi terperinci (Korea)](../INSTALL.md) untuk verifikasi hash,
pembaruan, perilaku uninstall, diagnostik, dan pemecahan masalah.

## Menggunakan monitor

Gunakan menu tray untuk me-refresh penggunaan, memilih interval refresh 1/5/10/15/30 menit, serta menampilkan atau menyembunyikan widget.
Menu ini juga menyediakan pengaturan startup Windows, tampilan startup, refresh autentikasi, refresh autentikasi otomatis, bahasa, dan diagnostik.
Pilih **Widget: all monitors** atau **Widget: primary monitor only** untuk mengontrol penempatan multi-monitor; pilihan ini diingat setelah restart.

Secara default, bahasa UI mengikuti locale Windows jika cocok dengan bahasa yang didukung. Anda juga dapat memilih bahasa secara manual dari menu tray. Bahasa yang didukung adalah Korea, Inggris, Spanyol, Portugis Brasil, Indonesia, Jepang, Hindi, Jerman, Prancis, Vietnam, Turki, dan Arab.

Widget taskbar menggunakan tema sistem terang/gelap Windows untuk teksnya dan membiarkan material taskbar native terlihat melalui latar belakangnya.

Hanya satu permintaan penggunaan yang berjalan pada satu waktu. Permintaan yang gagal dicoba ulang dengan jeda yang meningkat sementara nilai terakhir yang berhasil tetap terlihat.

Jika widget taskbar tidak dapat ditempelkan setelah Explorer dimulai ulang atau tata letak taskbar berubah, ikon tray tetap tersedia dan monitor mencoba ulang dengan aman.

## Privasi dan keamanan

Monitor tidak pernah membaca atau mem-parsing isi `%USERPROFILE%\.codex\auth.json`.
Diagnostik hanya memeriksa apakah path tersebut ada.

Respons RPC mentah diproses hanya cukup lama untuk mengekstrak jenis login dan kolom batas penggunaan yang ditampilkan.
Token, ID akun, alamat email, isi file autentikasi, dan nilai proxy tidak disimpan atau ditulis ke log.

Pengaturan disimpan di `%APPDATA%\CodexPeek\settings.json`.
Log diagnostik berbatas disimpan di `%TEMP%\codex-peek.log`.

Untuk panduan lengkap tentang penanganan data dan pelaporan kerentanan, lihat [SECURITY.md](../../SECURITY.md).

## Pemecahan masalah

| Masalah | Yang harus dilakukan |
| --- | --- |
| Codex CLI tidak ditemukan | Jalankan `codex --version` dan `where.exe codex`, lalu pastikan Codex CLI ada di `PATH`. |
| CLI tidak didukung | Perbarui Codex CLI. Dukungan RPC yang diperlukan lebih penting daripada nomor versi yang ditampilkan. |
| Keluar atau autentikasi kedaluwarsa | Selesaikan alur login normal di Codex CLI, lalu pilih **Refresh authentication** di menu tray. |
| Widget taskbar berada di monitor yang salah | Pilih **Widget: all monitors** atau **Widget: primary monitor only** dari menu tray. |
| Widget taskbar hilang | Gunakan widget mengambang atau ikon tray, mulai ulang Explorer jika perlu, lalu pilih mode monitor widget yang diinginkan. |
| Perlu detail lebih lanjut | Jalankan `--diagnose` atau buka **Diagnostics** dari menu tray. |

## Pengembangan

Build sumber memerlukan Rust 1.85 atau lebih baru, Visual Studio 2022 C++ Build Tools, dan
Windows SDK. Build dan validasi dari root repositori:

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Pemeriksaan otomatis tidak menggantikan skenario pemulihan Windows, DPI, multi-monitor, dan Explorer dalam [checklist release](../RELEASE_CHECKLIST.md).

## ❤️ Dukungan

Jika CodexPeek menghemat waktu Anda, pertimbangkan untuk mendukung pengembangannya.

- ⭐ Beri bintang pada repositori ini
- ❤️ [Sponsor di GitHub](https://github.com/sponsors/lch5518)

Setiap sponsor membantu menjaga proyek ini tetap aktif dipelihara.

## Lisensi

Proyek ini tersedia di bawah [MIT License](../../LICENSE).
Lihat [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) untuk pemberitahuan pihak ketiga.

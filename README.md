# Awan Host Remote

Aplikasi desktop **Rust native** untuk menghubungkan dan mengelola server Linux lewat SSH: terminal interaktif, file manager dan editor remote, Docker, database di container, firewall, serta instalasi aplikasi — semuanya dari satu jendela, tanpa Electron, WebView, atau browser.

> **🤖 Dibangun 100% dengan bantuan AI.**
> Seluruh kode aplikasi ini — arsitektur, UI, koneksi SSH, skrip server, hingga dokumentasi ini — ditulis oleh AI coding assistant. Pemilik proyek berperan menentukan arah dan fitur, meninjau hasil, serta menjalankan build dan pengujian sendiri. Kode belum diaudit secara independen; tinjau dan uji terlebih dahulu sebelum dipakai pada server produksi.

## Fitur

### Hosts dan koneksi
- Tambah host dengan nama, IP/hostname, port SSH, username, dan autentikasi **password** atau **private key** (dengan passphrase).
- **Verifikasi host key**: host baru menampilkan fingerprint SHA-256 untuk dicocokkan; host key yang berubah ditolak.
- **Login tersimpan** di Windows Credential Manager atau keyring Linux (GNOME Keyring/KWallet via Secret Service), bukan file plaintext, satu profil per alamat IP. Login yang gagal tidak menimpa login lama.
- Beberapa host dapat terhubung bersamaan; sidebar menampilkan setiap host aktif beserta halamannya.

### Summary
Identitas server (hostname, OS, kernel, uptime), pemakaian CPU, RAM, dan disk, serta tombol **Update System**, **Reboot**, dan **Shutdown** dengan konfirmasi.

### Terminal
- Terminal interaktif penuh (emulator `alacritty_terminal`): warna ANSI, keyboard, resize PTY, vim, htop, dan aplikasi layar penuh lainnya.
- Panel terminal di bagian bawah setiap halaman: tinggi dapat ditarik, bisa dibuka layar penuh, dan menu klik kanan Salin/Tempel.

### File Editor
- **Explorer** bertingkat untuk folder remote; klik area kosong untuk kembali menargetkan folder Home.
- Buat file/folder, **import** file atau folder dari komputer (maks. 25 MiB per file), salin, tempel, ganti nama, **pindahkan** (pemilih folder tujuan), hapus, dan **chmod** (grid Pemilik/Grup/Lainnya × Baca/Tulis/Eksekusi, preset, rekursif).
- Editor teks dengan syntax highlighting, status tersimpan/belum, dan pemeriksaan konflik sebelum menyimpan.
- **Preview Markdown** untuk file `.md`.
- **Git**: clone repository, panduan login Git via SSH (GitHub/GitLab) yang menampilkan username, serta Status/Pull/Push pada folder repository.
- Buka folder di terminal, dan jalankan `docker compose up -d --build` langsung dari file compose.

### Docker
- Daftar container dikelompokkan per **project Compose**, dengan status, image, dan port.
- **Pemakaian CPU dan RAM** per container, total keseluruhan, serta urutan berdasarkan CPU atau RAM tertinggi untuk melihat container yang paling berat.
- Aksi: lihat **log** (follow), **restart**, **stop/start**, **exec** perintah, **reload** konfigurasi Caddy/Nginx, **rebuild** per service atau satu project, **down**, dan **buat network** (mis. `edge`). Aksi yang mengganggu layanan selalu meminta konfirmasi.
- **Backup database** (MariaDB/MySQL/PostgreSQL) langsung **diunduh ke komputer Anda** — tidak ada file yang ditinggalkan di server. Password diambil dari variabel environment container itu sendiri.
- Bantuan satu klik bila akun SSH belum masuk grup `docker`.

### Database Browser
Dibuka dari tombol tabel pada container database di halaman Docker.
- Kiri: pohon **database → tabel** dengan perkiraan jumlah baris.
- Kanan, tab **Data**: tabel data (virtualized, kolom bisa diubah lebarnya, Shift + scroll untuk geser ke samping) dengan pagination 100 baris.
- Tab **SQL**: editor query dengan template (SELECT, CREATE TABLE, INSERT, UPDATE, DELETE). Query baca dijalankan dalam sesi read-only; **DDL/DML selalu meminta konfirmasi**, dengan peringatan khusus untuk DROP/TRUNCATE/DELETE/ALTER. Di PostgreSQL seluruh skrip berjalan dalam satu transaksi.

### Firewall
Status UFW/firewalld, port yang listening, serta tambah/hapus aturan port dengan konfirmasi dan peringatan bila perubahan dapat memutus akses SSH.

### Applications
Katalog instalasi (Nginx, Docker Engine, Docker Compose, Git, Node.js, Redis, PostgreSQL) dan daftar seluruh paket terpasang yang dapat dicari, dengan install/uninstall melalui APT atau DNF beserta log proses.

### Pengaturan
Tema terang/gelap dan bahasa tampilan (**Indonesia / English**).

## Prinsip yang dipegang

- **Data nyata saja**: semua halaman membaca dari server yang terhubung; tidak ada host demo atau data contoh. Informasi yang tidak bisa diambil ditampilkan sebagai tidak tersedia beserta penyebabnya.
- **Tidak ada kredensial plaintext**: login SSH di Credential Manager / keyring OS; password database tidak pernah diketik, ditampilkan, atau disimpan oleh aplikasi.
- **Konfirmasi untuk tindakan berisiko**: reboot, shutdown, stop/restart/rebuild/down container, perubahan firewall, penghapusan file, dan query yang mengubah database.
- **Ringan dan responsif**: semua operasi jaringan berjalan di background dengan channel SSH terpisah per jenis pekerjaan, buffer dan output dibatasi, daftar besar hanya merender baris yang terlihat, dan polling berhenti saat halaman tidak aktif atau host terputus.

## Kebutuhan

**Komputer lokal**
- Rust **1.89+** (edition 2024) dan toolchain native platform. Di Windows dengan MSVC: Microsoft C++ Build Tools dan Windows SDK.
- Di Ubuntu/Linux: driver Vulkan dan library X11/Wayland (lihat [Ubuntu / Linux](#ubuntu--linux)).
- Login tersimpan tersedia di **Windows** (Credential Manager) dan **Linux** (keyring desktop via Secret Service). Tanpa keyring yang berjalan, atau di macOS, login manual tetap bisa dipakai.

**Server**
- Linux dengan SSH.
- **Python 3** untuk Summary, Firewall, dan operasi file/folder.
- **Docker** (dan Docker Compose) untuk halaman Docker; `mariadb`/`mysql` atau `psql` di dalam container database untuk Database Browser dan backup.
- APT atau DNF untuk halaman Applications. Tindakan yang membutuhkan root memakai `sudo`.

Aplikasi tidak pernah memasang tool di server secara otomatis; instalasi hanya terjadi bila diminta dari halaman Applications.

## Menjalankan

```powershell
cargo run            # build debug lalu buka aplikasi
cargo run --release  # versi teroptimasi
```

Pemakaian pertama mengunduh dependency dan melakukan build, jadi butuh waktu lebih lama.

### Ubuntu / Linux

Build langsung di Ubuntu (atau WSL2 dengan WSLg); cross-compile dari Windows tidak didukung.

```bash
sudo apt install build-essential pkg-config clang cmake   libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev   libx11-xcb-dev libxcb1-dev libfontconfig-dev libfreetype-dev   libvulkan1 mesa-vulkan-drivers
cargo run --release
```

Login tersimpan memerlukan keyring desktop yang berjalan (`gnome-keyring` atau KWallet); jika keyring terkunci, sistem akan meminta Anda membukanya.

Paket `.deb` (berisi binary, entri menu aplikasi, dan ikon):

```bash
cargo install cargo-deb
cargo deb                                   # hasil: target/debian/awan-host-remote_<versi>_amd64.deb
sudo apt install ./target/debian/awan-host-remote_*.deb
```

## Teknologi

| Bagian | Library |
|---|---|
| UI native | [gpui-kit](https://crates.io/crates/gpui-kit) (gpui + gpui-component, ikon Lucide) |
| SSH | [russh](https://crates.io/crates/russh) |
| Runtime async | [Tokio](https://tokio.rs) |
| Emulator terminal | [alacritty_terminal](https://crates.io/crates/alacritty_terminal) |
| Pembersihan rahasia di memori | [zeroize](https://crates.io/crates/zeroize) |

## Struktur kode

| File | Isi |
|---|---|
| `src/main.rs` | Entry point dan pembukaan jendela. |
| `src/app.rs` | Seluruh UI: sidebar, halaman, dialog, dan alur antar fitur. |
| `src/model.rs` | Daftar halaman dan model host. |
| `src/i18n.rs` | Teks UI bahasa Indonesia dan Inggris. |
| `src/ssh.rs` | Sesi SSH, verifikasi host key, PTY, dan channel background per jenis pekerjaan. |
| `src/terminal.rs` | Status sesi per host dan antrean permintaan ke SSH. |
| `src/terminal_widget.rs` | Widget terminal (render grid, keyboard, salin/tempel). |
| `src/host_store.rs` | Normalisasi alamat dan penyimpanan login di Windows Credential Manager / Secret Service (Linux). |
| `src/live.rs`, `src/live_remote.py` | Pembacaan Summary/Firewall dan tindakan server (firewall, reboot/shutdown). |
| `src/remote_file.rs` | Baca/simpan file remote dengan pemeriksaan revisi dan penulisan atomik. |
| `src/remote_directory.rs`, `.py` | Daftar isi folder remote. |
| `src/remote_fs.rs` | Operasi file manager: buat, hapus, ganti nama, salin, unggah, chmod. |
| `src/git_auth.rs` | Panduan login Git via SSH. |
| `src/packages.rs`, `src/system_update.rs` | Inventaris paket, install/uninstall, update sistem, git clone. |
| `src/docker.rs` | Daftar container, statistik CPU/RAM, dan aksi Docker. |
| `src/docker_access.rs` | Pemeriksaan akses Docker (grup `docker`). |
| `src/docker_db.rs` | Query database di container untuk Database Browser. |
| `build.rs` | Stack thread utama 8 MB, ikon, dan info versi di Windows. |
| `packaging/linux/` | Entri `.desktop` untuk paket `.deb`. |

`editor.rs`, `explorer.rs`, `live_views.rs`, `saved_hosts.rs`, dan `views.rs` adalah sisa versi UI lama yang tidak lagi dikompilasi.

## Status

Proyek masih dalam pengembangan aktif. Fitur ditulis oleh AI dan diuji secara manual oleh pemilik proyek; belum ada test otomatis. Gunakan server uji terlebih dahulu untuk tindakan yang mengubah sistem (firewall, paket, Docker, query DDL/DML).

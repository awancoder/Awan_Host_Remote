/**
 * ============================================================================
 * .awanapps / apps.js — Template Metadata Aplikasi untuk Awan Digitals
 * ============================================================================
 *
 * File ini berisi metadata aplikasi yang nantinya akan ditambahkan ke array
 * `apps.json` di root repositori `awanapps` (https://apps.awandigitals.com).
 *
 * Isi dengan data ASLI dari codebase project ini — jangan mengarang.
 * Setelah selesai, jalankan: `node .awanapps/validate.js`
 *
 * ----------------------------------------------------------------------------
 * PANDUAN PENGISIAN FIELD:
 * ----------------------------------------------------------------------------
 * 1. slug (WAJIB):
 *    - ID unik aplikasi dalam huruf kecil dan tanda strip (kebab-case).
 *    - Digunakan sebagai URL halaman detail: `https://apps.awandigitals.com/{slug}`
 *    - Contoh: "gerak", "coretan", "cookies-list-manager", "awan-pdf"
 *    - Pastikan belum dipakai app lain di `apps.json` repo awanapps.
 *
 * 2. name (WAJIB):
 *    - Objek multibahasa: `{ id: "Nama ID", en: "Nama EN" }`
 *    - Jika nama sama di kedua bahasa, isi nilai yang sama pada `id` dan `en`.
 *
 * 3. category (WAJIB, string tunggal):
 *    - Kategori yang SUDAH punya styling badge warna di `src/app.css` awanapps:
 *      - "Utilities"
 *      - "Productivity"
 *      - "Developer Tool"
 *      - "Health"
 *      - "Accounting"
 *      - "Media Social"
 *      - "Marketing & Sales"
 *      - "Finance"
 *      - "Game"
 *      - "Design"
 *    - Kategori yang SUDAH DIPAKAI di katalog tapi BELUM punya badge warna
 *      (badge tetap tampil dengan warna netral — tidak error):
 *      - "Health & Fitness"
 *      - "Video Editor"
 *      - "Tools"
 *      - "Education"
 *      - "Travel & Local"
 *    - Boleh memakai kategori baru di luar daftar ini, tapi saat merge
 *      tambahkan class `.badge-cat-{kategori-kebab-case}` di `src/app.css`
 *      agar badge tidak tampil netral.
 *
 * 4. label (WAJIB, array string):
 *    - Platform aplikasi. Nilai standar (semuanya sudah punya badge warna):
 *      - "Web App"
 *      - "Desktop"
 *      - "Mobile"
 *      - "Extension"
 *      - "Game"
 *    - Boleh lebih dari 1 nilai, misal: ["Extension", "Web App"]
 *    - Label baru perlu class `.badge-val-{label-kebab-case}` di `src/app.css`.
 *
 * 5. description (WAJIB):
 *    - Paragraf lengkap yang menjelaskan fungsi, fitur utama, dan keunggulan aplikasi.
 *    - Objek multibahasa: `{ id: "...", en: "..." }`
 *    - Tulis berdasarkan fitur yang BENAR-BENAR ada di codebase.
 *
 * 6. shortDescription (WAJIB):
 *    - Ringkasan singkat (1 kalimat padat) untuk card listing di halaman utama.
 *    - Objek multibahasa: `{ id: "...", en: "..." }`
 *
 * 7. icon (WAJIB, boleh null):
 *    - Rekomendasi: path lokal di repo awanapps `/assets/logo/{slug}.png`
 *      (nama file mengikuti slug agar mudah dilacak).
 *    - Standar asset: PNG transparan, rasio 1:1 (rekomendasi 512x512 px),
 *      nantinya disalin ke `public/assets/logo/{slug}.png` di repo awanapps.
 *    - Boleh juga URL absolut `https://...` (misal favicon dari domain app sendiri).
 *    - Jika diisi `null` (atau gambar gagal dimuat), UI otomatis menampilkan
 *      kotak fallback berisi huruf inisial nama aplikasi — halaman tetap normal.
 *
 * 8. image (Opsional):
 *    - URL gambar banner / preview / screenshot (rasio 2:1 atau 16:9).
 *    - Dipakai sebagai `og:image` & `twitter:image` di halaman detail.
 *      Jika kosong, otomatis fallback ke `icon`.
 *    - Isi `null` atau hilangkan field-nya jika belum ada. JANGAN kirim
 *      URL placeholder (placehold.co) ke produksi.
 *
 * 9. Link Distribusi & Toko Aplikasi (Opsional, isi null / hilangkan jika tidak ada):
 *    - website        : URL landing page / live web app (misal: "https://nama-app.awandigitals.com")
 *    - googlePlay     : URL listing Google Play Store
 *    - appStore       : URL listing Apple App Store
 *    - download       : URL download installer langsung / GitHub Releases (desktop/APK)
 *    - chromeWebStore : URL listing Chrome Web Store (extension)
 *    - mozillaAddOn   : URL listing Firefox Add-ons (extension)
 *    - Semua URL harus diawali `https://`. Kartu "Unduh & Akses" di halaman
 *      detail hanya muncul kalau minimal salah satu dari link di atas terisi.
 *
 * 10. Link Komunitas (Opsional, isi null / hilangkan jika tidak ada):
 *     - facebookGroup   : URL grup Facebook komunitas
 *     - telegramChannel : URL channel Telegram resmi
 *     - telegramGroup   : URL grup diskusi Telegram
 *     - Section "Komunitas" hanya muncul kalau minimal salah satu terisi.
 *
 * 11. pricing (Opsional, string, default: "free"):
 *     - "free" (default) : Gratis — tidak perlu ditulis jika aplikasi gratis.
 *     - "ads"            : Berisi Iklan (tulis `"pricing": "ads"`).
 *     - "paid"           : Berbayar (tulis `"pricing": "paid"`).
 *
 * 12. is_access_login (Opsional, boolean, default: false):
 *     - Tulis `"is_access_login": true` HANYA jika aplikasi memerlukan akun/login.
 *     - Jika tanpa login, field ini tidak perlu ditulis (otomatis "Tanpa Login").
 *
 * 13. releaseDate (Opsional, boleh null):
 *     - Tanggal rilis versi publik, format "YYYY-MM-DD" (contoh: "2026-09-09").
 *     - Ambil dari tanggal rilis store / tag rilis git, bukan tanggal karangan.
 *
 * 14. privacyPolicyUrl (Opsional, default null):
 *     - "local"       : Konten dibaca dari `src/lib/privacy/{slug}.js` di repo awanapps
 *                       (isi file `.awanapps/privacy.js` ini).
 *     - "https://..." : Tombol membuka tautan kebijakan eksternal di tab baru.
 *     - null / tidak ditulis : Halaman `/{slug}/privacy-policy` tetap ada tapi
 *                       memakai teks kebijakan generik Awan Digitals.
 *
 * 15. howToUseUrl (Opsional, default null):
 *     - "local"       : Konten dibaca dari `src/lib/how-to-use/{slug}.js` di repo awanapps
 *                       (isi file `.awanapps/how-to-use.js` ini).
 *     - "https://..." : Tombol membuka tautan panduan eksternal di tab baru.
 *     - null / tidak ditulis : Kartu panduan tidak ditampilkan, halaman
 *                       `/{slug}/how-to-use` mengembalikan 404 dan tidak masuk sitemap.
 *     - Sebagian besar app di katalog TIDAK memakai field ini — isi "local"
 *       hanya kalau memang menulis panduan di `.awanapps/how-to-use.js`.
 * ============================================================================
 */

export default [
  {
    "slug": "awan-host-remote",
    "name": {
      "id": "Awan Host Remote",
      "en": "Awan Host Remote"
    },
    "category": "Developer Tool",
    "label": [
      "Desktop"
    ],
    "description": {
      "id": "Awan Host Remote adalah aplikasi desktop Rust native untuk menghubungkan dan mengelola server Linux melalui SSH dari satu jendela, tanpa Electron, WebView, atau browser. Tambahkan host dengan autentikasi password atau private key, verifikasi fingerprint host key SHA-256, dan simpan login secara aman di Windows Credential Manager. Halaman Summary menampilkan hostname, OS, kernel, uptime, serta pemakaian CPU, RAM, dan disk, lengkap dengan tombol Update System, Reboot, dan Shutdown berkonfirmasi. Terminal interaktif penuh berbasis alacritty_terminal mendukung warna ANSI, resize PTY, vim, dan htop. File Editor menyediakan Explorer folder remote, import file, salin/pindah/ganti nama/hapus, chmod, editor dengan syntax highlighting dan pemeriksaan konflik, preview Markdown, serta Git clone/status/pull/push. Halaman Docker mengelompokkan container per project Compose dengan statistik CPU/RAM, log, restart, rebuild, exec, dan backup database yang langsung diunduh ke komputer. Database Browser untuk MariaDB/MySQL/PostgreSQL di dalam container menampilkan tabel dengan pagination dan editor SQL baca-saja secara default. Tersedia juga pengelolaan firewall UFW/firewalld dengan peringatan akses SSH, serta katalog instalasi paket melalui APT atau DNF. Antarmuka tersedia dalam bahasa Indonesia dan Inggris dengan tema terang/gelap.",
      "en": "Awan Host Remote is a native Rust desktop app for connecting to and managing Linux servers over SSH from a single window, with no Electron, WebView, or browser. Add hosts with password or private key authentication, verify SHA-256 host key fingerprints, and store logins securely in Windows Credential Manager. The Summary page shows hostname, OS, kernel, uptime, and CPU, RAM, and disk usage, along with confirmed Update System, Reboot, and Shutdown actions. A full interactive terminal powered by alacritty_terminal supports ANSI colors, PTY resizing, vim, and htop. The File Editor provides a remote folder Explorer, file import, copy/move/rename/delete, chmod, an editor with syntax highlighting and conflict checks, Markdown preview, and Git clone/status/pull/push. The Docker page groups containers by Compose project with CPU/RAM stats, logs, restart, rebuild, exec, and database backups downloaded straight to your computer. The Database Browser for MariaDB/MySQL/PostgreSQL containers shows paginated tables and a SQL editor that is read-only by default. It also manages UFW/firewalld rules with SSH lockout warnings and offers a package install catalog via APT or DNF. The interface is available in Indonesian and English with light/dark themes."
    },
    "shortDescription": {
      "id": "Kelola server Linux lewat SSH: terminal, file editor, Docker, database, firewall, dan paket dalam satu aplikasi desktop native.",
      "en": "Manage Linux servers over SSH: terminal, file editor, Docker, databases, firewall, and packages in one native desktop app."
    },
    "icon": "/assets/logo/awan-host-remote.png",
    "image": null,
    "website": null,
    "googlePlay": null,
    "appStore": null,
    "download": "https://github.com/awancoder/Awan_Host_Remote/releases",
    "mozillaAddOn": null,
    "chromeWebStore": null,
    "facebookGroup": null,
    "telegramChannel": null,
    "telegramGroup": null,
    "releaseDate": "2026-10-10",
    "privacyPolicyUrl": "local",
    "howToUseUrl": null
  }
];

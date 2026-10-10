/**
 * ============================================================================
 * .awanapps / privacy.js — Template Kebijakan Privasi (Privacy Policy)
 * ============================================================================
 *
 * Diisi HANYA jika pada `apps.js` field `privacyPolicyUrl` disetel ke `"local"`.
 * Berisi teks Kebijakan Privasi multibahasa (ID & EN).
 *
 * Saat di-merge ke repo awanapps:
 * 1. Simpan isi file ini sebagai `src/lib/privacy/{slug}.js`
 * 2. GANTI blok komentar panduan ini dengan header pendek, contoh:
 *      "{Nama Aplikasi} - Privacy Policy Content / src/lib/privacy/{slug}.js"
 *    Jangan biarkan blok panduan template ini ikut ter-commit ke repo awanapps.
 * 3. Daftarkan di registry `src/lib/privacy/index.js`
 *
 * ----------------------------------------------------------------------------
 * STRUKTUR SECTION & FIELD:
 * ----------------------------------------------------------------------------
 * Setiap bahasa (`en` dan `id`) memiliki:
 * - `lastUpdated` (string) : Tanggal pembaruan kebijakan, ditulis natural sesuai
 *                            bahasanya ("September 9, 2026" / "9 September 2026").
 * - `sections` (array of objects) : Daftar pasal kebijakan dengan properti:
 *     - `title`   (string, WAJIB)   : Judul section (misal: "1. Overview", "1. Gambaran Umum").
 *     - `content` (string, Opsional): Paragraf penjelasan utama.
 *     - `points`  (array, Opsional) : Poin daftar berbutir (bullet list). Cocok untuk daftar permission/data.
 *     - `note`    (string, Opsional): Catatan kaki atau paragraf tambahan setelah bullet points.
 *     - `email`   (string, Opsional): Email dukungan resmi (dirender sebagai link mailto).
 *
 * CATATAN: field `youtube` TIDAK dirender di halaman privacy policy —
 * field itu khusus untuk `how-to-use.js`.
 *
 * Jumlah dan urutan section pada `en` dan `id` harus SAMA (isi saling terjemahan),
 * karena kedua bahasa dikirim bersamaan dan pengguna bisa bertukar bahasa tanpa reload.
 *
 * ----------------------------------------------------------------------------
 * PANDUAN PENTING UNTUK AI & DEVELOPER:
 * ----------------------------------------------------------------------------
 * - AKURASI LEGAL: Sesuaikan section dengan apa yang BENAR-BENAR dilakukan aplikasi saat ini.
 *   Jangan menyalin klaim "100% offline" kalau kode ternyata melakukan request jaringan.
 * - JIKA APLIKASI 100% OFFLINE: Jelaskan bahwa data tidak dikirim ke server mana pun dan hanya
 *   tersimpan di perangkat lokal (SharedPreferences, Room/SQLite, localStorage, IndexedDB, file lokal).
 * - JIKA BROWSER EXTENSION: Cantumkan izin dari `manifest.json` (`storage`, `tabs`, `activeTab`,
 *   `cookies`, `host_permissions`) beserta tujuan masing-masing.
 * - JIKA MOBILE: Cantumkan izin dari `AndroidManifest.xml` / iOS `Info.plist`
 *   (`POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`, `CAMERA`, `ACCESS_FINE_LOCATION`, dll) + alasannya.
 * - JIKA ADA AKUN/LOGIN, BACKEND, ATAU SINKRONISASI: Jelaskan data apa yang dikirim, ke mana,
 *   dan bagaimana pengguna bisa menghapus akun/datanya. Pastikan `is_access_login: true` di `apps.js`.
 * - JIKA ADA ANALYTICS / ADS: Sebutkan pihak ketiganya (Google Analytics, Firebase, AdMob) dan
 *   selaraskan dengan `pricing: "ads"` di `apps.js`. Jika TIDAK ADA, nyatakan secara tegas.
 * - Struktur section di bawah hanya kerangka umum — tambah/kurangi sesuai kebutuhan aplikasi.
 * ============================================================================
 */

export default {
  en: {
    lastUpdated: "October 10, 2026",
    sections: [
      {
        title: "1. Overview",
        content: "Awan Host Remote is a native desktop application developed by Awan Digitals for connecting to and managing your own Linux servers over SSH. The app has no Awan Digitals account, no backend server, and no cloud sync. This Privacy Policy explains what data the app handles on your computer and what it sends to the servers you connect to."
      },
      {
        title: "2. Data Stored on Your Computer",
        content: "Awan Host Remote stores only the minimum data needed to reconnect to your servers, and keeps it on your own computer:",
        points: [
          "Saved SSH logins — host name, address, SSH port, username, authentication method, private key file path, and the password or key passphrase. These are stored only if you choose to save a login, inside Windows Credential Manager (protected by your Windows account), never in a plaintext file. One profile is kept per normalized IP address.",
          "Known host keys — the SSH host key of each server you approve is recorded in a known_hosts file in your local application data folder (%LOCALAPPDATA%/awan-host-remote) so the app can detect if a server's identity changes.",
          "Files you choose to download — database backups are saved only to the location you pick in the Save dialog."
        ],
        note: "You can delete a saved login from the Hosts page (with confirmation), or remove it directly in Windows Credential Manager. Deleting the awan-host-remote folder removes the known host records. Passwords and private key contents are cleared from memory after use and are never written to logs or error messages."
      },
      {
        title: "3. Data Sent to Your Servers",
        content: "All network traffic goes only to the SSH servers you add, over an encrypted SSH connection. Depending on the features you use, the app sends:",
        points: [
          "Your SSH credentials, for authentication with that server only.",
          "Commands and keystrokes you enter in the terminal, and read-only scripts that collect system information (CPU, RAM, disk, firewall, ports, Docker, packages).",
          "File contents you edit, create, or import from your computer (up to 25 MiB per file).",
          "SQL queries you run in the Database Browser, executed inside the database container on your server."
        ],
        note: "Database passwords are read from the container's own environment variables on the server; the app never asks you to type, display, or store them. Actions that change your server (reboot, shutdown, firewall rules, package changes, container actions, file deletion, and data-modifying SQL) always ask for confirmation first."
      },
      {
        title: "4. Computer Permissions & Access",
        content: "As a desktop application, Awan Host Remote does not request special operating system permissions. It uses only the following resources:",
        points: [
          "Network access — to open SSH connections to the servers you add.",
          "Windows Credential Manager — to save and read the SSH logins you choose to keep.",
          "Local file access — to read private key files you select, import files or folders you pick, and save backups to the location you choose.",
          "Clipboard — used only when you copy or paste in the terminal."
        ]
      },
      {
        title: "5. Third-Party Services, Analytics, and Advertising",
        content: "Awan Host Remote contains no analytics, telemetry, crash reporting, or advertising SDKs, and does not contact Awan Digitals or any third-party service. We do not collect, sell, rent, or trade your data. Third-party services are reached only when you explicitly request it on your own server, for example when the server's package manager (APT/DNF) downloads packages, or when Git clones, pulls, or pushes to a repository you specify."
      },
      {
        title: "6. Your Responsibility",
        content: "You are responsible for having authorization to access the servers you connect to and for the commands you run. The app acts only with the permissions of your SSH account; actions that require root use sudo on your server."
      },
      {
        title: "7. Children's Privacy",
        content: "Awan Host Remote is a server administration tool and is not directed at children under the age of 13. It does not knowingly collect personal information from children."
      },
      {
        title: "8. Changes to This Privacy Policy",
        content: "We may update this Privacy Policy from time to time. Any changes become effective immediately upon posting to this page with an updated 'Last Updated' date."
      },
      {
        title: "9. Contact Us",
        content: "If you have any questions, feedback, or concerns regarding this Privacy Policy, please contact our support team at:",
        email: "support@awandigitals.com"
      }
    ]
  },
  id: {
    lastUpdated: "10 Oktober 2026",
    sections: [
      {
        title: "1. Gambaran Umum",
        content: "Awan Host Remote adalah aplikasi desktop native yang dikembangkan oleh Awan Digitals untuk menghubungkan dan mengelola server Linux milik Anda sendiri melalui SSH. Aplikasi ini tidak memiliki akun Awan Digitals, server backend, maupun sinkronisasi cloud. Kebijakan Privasi ini menjelaskan data apa yang dikelola aplikasi di komputer Anda dan apa yang dikirim ke server yang Anda hubungkan."
      },
      {
        title: "2. Data yang Disimpan di Komputer Anda",
        content: "Awan Host Remote hanya menyimpan data seminimal mungkin untuk menyambung ulang ke server Anda, dan data tersebut tetap berada di komputer Anda:",
        points: [
          "Login SSH tersimpan — nama host, alamat, port SSH, username, metode autentikasi, path file private key, serta password atau passphrase key. Data ini hanya disimpan jika Anda memilih menyimpan login, di dalam Windows Credential Manager (dilindungi akun Windows Anda), tidak pernah dalam file plaintext. Satu profil disimpan untuk setiap alamat IP yang dinormalisasi.",
          "Host key yang dikenal — host key SSH setiap server yang Anda setujui dicatat dalam file known_hosts di folder data aplikasi lokal (%LOCALAPPDATA%/awan-host-remote) agar aplikasi dapat mendeteksi jika identitas server berubah.",
          "File yang Anda pilih untuk diunduh — backup database hanya disimpan di lokasi yang Anda pilih pada dialog Simpan."
        ],
        note: "Anda dapat menghapus login tersimpan dari halaman Hosts (dengan konfirmasi), atau langsung melalui Windows Credential Manager. Menghapus folder awan-host-remote akan menghapus catatan host yang dikenal. Password dan isi private key dibersihkan dari memori setelah dipakai dan tidak pernah ditulis ke log maupun pesan error."
      },
      {
        title: "3. Data yang Dikirim ke Server Anda",
        content: "Seluruh lalu lintas jaringan hanya menuju server SSH yang Anda tambahkan, melalui koneksi SSH terenkripsi. Bergantung pada fitur yang Anda gunakan, aplikasi mengirim:",
        points: [
          "Kredensial SSH Anda, hanya untuk autentikasi ke server tersebut.",
          "Perintah dan ketikan yang Anda masukkan di terminal, serta skrip baca-saja untuk mengambil informasi sistem (CPU, RAM, disk, firewall, port, Docker, paket).",
          "Isi file yang Anda edit, buat, atau import dari komputer (maksimal 25 MiB per file).",
          "Query SQL yang Anda jalankan di Database Browser, dieksekusi di dalam container database pada server Anda."
        ],
        note: "Password database dibaca dari variabel environment container itu sendiri di server; aplikasi tidak pernah meminta Anda mengetik, menampilkan, atau menyimpannya. Tindakan yang mengubah server (reboot, shutdown, aturan firewall, perubahan paket, aksi container, penghapusan file, dan SQL yang mengubah data) selalu meminta konfirmasi terlebih dahulu."
      },
      {
        title: "4. Izin & Akses Komputer",
        content: "Sebagai aplikasi desktop, Awan Host Remote tidak meminta izin khusus sistem operasi. Aplikasi hanya menggunakan sumber daya berikut:",
        points: [
          "Akses jaringan — untuk membuka koneksi SSH ke server yang Anda tambahkan.",
          "Windows Credential Manager — untuk menyimpan dan membaca login SSH yang Anda pilih untuk disimpan.",
          "Akses file lokal — untuk membaca file private key yang Anda pilih, mengimpor file atau folder pilihan Anda, dan menyimpan backup ke lokasi pilihan Anda.",
          "Clipboard — hanya digunakan saat Anda menyalin atau menempel di terminal."
        ]
      },
      {
        title: "5. Layanan Pihak Ketiga, Analitik, dan Iklan",
        content: "Awan Host Remote tidak berisi SDK analitik, telemetri, pelaporan crash, maupun iklan, dan tidak menghubungi Awan Digitals atau layanan pihak ketiga mana pun. Kami tidak mengumpulkan, menjual, menyewakan, atau memperdagangkan data Anda. Layanan pihak ketiga hanya dihubungi ketika Anda memintanya secara eksplisit di server Anda sendiri, misalnya saat package manager server (APT/DNF) mengunduh paket, atau saat Git melakukan clone, pull, atau push ke repository yang Anda tentukan."
      },
      {
        title: "6. Tanggung Jawab Anda",
        content: "Anda bertanggung jawab memastikan memiliki izin untuk mengakses server yang Anda hubungkan serta atas perintah yang Anda jalankan. Aplikasi hanya bertindak sesuai hak akses akun SSH Anda; tindakan yang membutuhkan root menggunakan sudo di server Anda."
      },
      {
        title: "7. Privasi Anak",
        content: "Awan Host Remote adalah alat administrasi server dan tidak ditujukan untuk anak-anak di bawah usia 13 tahun. Aplikasi ini tidak dengan sengaja mengumpulkan informasi pribadi dari anak-anak."
      },
      {
        title: "8. Perubahan Kebijakan Privasi",
        content: "Kami dapat memperbarui Kebijakan Privasi ini dari waktu ke waktu. Setiap perubahan berlaku segera setelah dipublikasikan pada halaman ini dengan tanggal 'Terakhir Diperbarui' yang disesuaikan."
      },
      {
        title: "9. Hubungi Kami",
        content: "Jika Anda memiliki pertanyaan, masukan, atau kendala mengenai Kebijakan Privasi ini, silakan hubungi tim kami melalui:",
        email: "support@awandigitals.com"
      }
    ]
  }
};

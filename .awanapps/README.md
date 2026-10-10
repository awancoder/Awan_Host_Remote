# .awanapps — Kit Pendaftaran & Integrasi Aplikasi ke Awan Digitals

Folder `.awanapps` adalah kit pendaftaran portabel yang dapat disalin ke **repositori aplikasi apa pun** (mobile, web app, browser extension, desktop app, game, tools).

Kit ini dirancang agar developer atau **AI coding assistant** di proyek tersebut dapat mengekstrak data asli aplikasi secara akurat, mengisi file metadata, memvalidasinya, dan mempermudah proses penggabungan (*merge*) ke repositori utama katalog [Awan Apps](https://apps.awandigitals.com).

> **Kebutuhan:** Node.js 18+ (hanya untuk menjalankan validator). Tidak ada dependensi lain — `npm install` tidak diperlukan.

---

## ⚡ Alur Cepat (Quick Start)

```
[Proyek Aplikasi Lain]                     [Repo awanapps]
      │                                          │
      ├─ 1. Copy folder `.awanapps` ke sini      │
      ├─ 2. Beri prompt ke AI / isi data         │
      ├─ 3. Jalankan `node .awanapps/validate.js`│
      └─────────────────────────────────────────► 4. Merge file & icon ke awanapps
```

1. **Salin** seluruh folder `.awanapps` ini ke root proyek aplikasi Anda.
2. **Berikan instruksi** ke AI Anda menggunakan [Prompt Siap Pakai](#-prompt-siap-pakai-untuk-ai-copy-paste) di bawah (atau isi manual).
3. **Validasi** data dengan menjalankan:
   ```bash
   node .awanapps/validate.js
   ```
   Validator sengaja **gagal (exit 1)** selama masih ada teks placeholder bawaan template — jadi data karangan/setengah jadi tidak akan lolos.
4. **Merge** hasil file ke repo `awanapps` mengikuti [Panduan Merge](#-panduan-merge-ke-repo-awanapps).

---

## 🤖 Prompt Siap Pakai untuk AI (Copy-Paste)

Cukup salin teks di bawah ini dan kirimkan ke AI assistant Anda (Antigravity, Cursor, ChatGPT, Claude, Gemini, Copilot, dll) di repositori proyek Anda:

### 🇮🇩 Prompt Bahasa Indonesia
```markdown
Pelajari codebase proyek ini secara menyeluruh, lalu perbarui file di dalam folder `.awanapps` (`apps.js`, `privacy.js`, dan `how-to-use.js`) dengan data ASLI dan akurat dari proyek ini:

1. Baca panduan field dan schema pada `.awanapps/apps.js`, `.awanapps/privacy.js`, dan `.awanapps/how-to-use.js`.
2. Ekstrak data dari package.json / pubspec.yaml / AndroidManifest.xml / manifest.json / README.md / source code fitur utama.
3. Tulis deskripsi, privacy policy, dan how-to-use secara bilingual (Bahasa Indonesia & English) dengan jumlah section yang sama persis di kedua bahasa.
4. Pastikan privacy policy mencerminkan arsitektur data sesungguhnya (offline/local storage, permission yang diminta, ada/tidaknya backend, analytics, dan iklan).
5. Isi `howToUseUrl: "local"` HANYA jika kamu memang menulis panduan di `.awanapps/how-to-use.js`; kalau tidak, set `null`.
6. Jangan menyisakan satu pun teks placeholder bawaan template.
7. Jalankan `node .awanapps/validate.js` sampai Total Error = 0.
```

### 🇬🇧 English Prompt
```markdown
Thoroughly examine this project codebase, then update the template files inside the `.awanapps` folder (`apps.js`, `privacy.js`, and `how-to-use.js`) with REAL, accurate data from this project:

1. Review the field schemas and guidelines in `.awanapps/apps.js`, `.awanapps/privacy.js`, and `.awanapps/how-to-use.js`.
2. Extract metadata and permissions from package.json / pubspec.yaml / AndroidManifest.xml / manifest.json / README.md / source code.
3. Provide bilingual content (Indonesian and English) for all text fields, with an identical number of sections in both languages.
4. Ensure the privacy policy reflects the actual data handling (local storage, network requests, device permissions, backend, analytics, ads).
5. Set `howToUseUrl: "local"` ONLY if you actually write the guide in `.awanapps/how-to-use.js`; otherwise set it to `null`.
6. Leave no template placeholder text behind.
7. Run `node .awanapps/validate.js` until it reports 0 errors.
```

---

## 📂 Struktur File Kit `.awanapps`

| File | Deskripsi |
|---|---|
| [`apps.js`](./apps.js) | Metadata utama aplikasi (slug, nama, kategori, label, deskripsi, tautan rilis, dsb). |
| [`privacy.js`](./privacy.js) | Kebijakan privasi lengkap bilingual (`id` & `en`) — diisi jika `privacyPolicyUrl: "local"`. |
| [`how-to-use.js`](./how-to-use.js) | Panduan langkah penggunaan bilingual (`id` & `en`) — diisi jika `howToUseUrl: "local"`. |
| [`validate.js`](./validate.js) | Skrip pengujian otomatis mandiri (*zero-dependency*) untuk memverifikasi data. |
| [`package.json`](./package.json) | Menandai folder ini sebagai ESM agar validator jalan di proyek non-Node (Flutter, Android, dll). Tidak perlu di-merge ke repo awanapps. |
| `README.md` | Dokumentasi dan petunjuk teknis kit ini. |

---

## 🔍 Panduan Penggalian Data Proyek untuk AI

Untuk memastikan akurasi data tanpa mengarang (*no hallucination*), AI harus memeriksa file-file berikut sesuai ekosistem proyek:

- **Browser Extension (Chrome / Firefox / Edge)**:
  - `manifest.json`: Periksa `name`, `version`, `description`, `permissions`, `host_permissions`, `icons`.
  - Content script & background service worker: Cek apakah ada komunikasi jaringan atau murni `chrome.storage.local`.
- **Android Native / Kotlin / Java**:
  - `app/build.gradle(.kts)` & `AndroidManifest.xml`: Periksa `applicationId`, `versionName`, daftar `<uses-permission>`.
  - Cek implementasi storage (SharedPreferences, DataStore, Room/SQLite) dan foreground services.
- **Flutter / Dart**:
  - `pubspec.yaml`: Nama, deskripsi, versi, dependensi plugin (camera, geolocator, shared_preferences, hive).
- **React Native / Expo**:
  - `app.json` / `package.json`: Bundle identifier, permissions, konfigurasi plugin.
- **Web App (Vite / Next.js / SvelteKit / Nuxt)**:
  - `package.json`, `index.html`, metadata layout / SEO headers, API routes, dan local storage.
- **Desktop (Tauri / Electron / Flutter Desktop / PyQt)**:
  - `tauri.conf.json` / `package.json` / file konfigurasi rilis build.
- **Asset Visual (Icon)**:
  - Siapkan icon aplikasi format **PNG transparan rasio 1:1** (rekomendasi **512x512 px**) untuk nantinya disalin ke `public/assets/logo/{slug}.png` di repo `awanapps`.
  - Jika icon belum ada, isi `"icon": null` — UI menampilkan kotak fallback berisi huruf inisial nama aplikasi (halaman tetap normal).

---

## 📋 Kamus Data & Nilai Standar

### 1. Kategori (`category`)
String tunggal. Kategori berikut **sudah punya badge warna** di `src/app.css` repo `awanapps`:

`Utilities` · `Productivity` · `Developer Tool` · `Health` · `Accounting` · `Media Social` · `Marketing & Sales` · `Finance` · `Game` · `Design`

Kategori berikut **sudah dipakai di katalog tapi belum punya badge warna** (badge tampil netral — tidak error):

`Health & Fitness` · `Video Editor` · `Tools` · `Education` · `Travel & Local`

Boleh memakai kategori baru, tetapi saat merge tambahkan class `.badge-cat-{kategori-kebab-case}` di `src/app.css` (misal `Travel & Local` → `.badge-cat-travel-local`).

### 2. Label Platform (`label`)
Array string platform aplikasi (bisa lebih dari satu, misal: `["Extension", "Web App"]`). Semua nilai berikut sudah punya badge warna:

`Web App` · `Desktop` · `Mobile` · `Extension` · `Game`

Label baru memerlukan class `.badge-val-{label-kebab-case}` di `src/app.css`.

### 3. Opsi Biaya & Akses Login (`pricing` & `is_access_login`)
Field ini opsional dengan nilai default:
- `pricing`:
  - `"free"` *(default)* : **Gratis** — tidak perlu ditulis, otomatis tampil *"Gratis"* / *"Free"*.
  - `"ads"` : **Berisi Iklan** — otomatis tampil *"Berisi Iklan"* / *"Contains Ads"*.
  - `"paid"` : **Berbayar** — otomatis tampil *"Berbayar"* / *"Paid"*.
- `is_access_login`: Tulis `"is_access_login": true` hanya jika memerlukan login/akun. Tanpa akun **tidak perlu ditulis** (otomatis *"Tanpa Login"* / *"No Sign-in"*).

Konsistensi: `pricing: "ads"` harus sejalan dengan penyebutan SDK iklan di `privacy.js`, dan `is_access_login: true` harus sejalan dengan penjelasan akun/backend di `privacy.js`.

### 4. Mode Kebijakan Privasi & Panduan (`privacyPolicyUrl` & `howToUseUrl`)
Keduanya opsional (default `null`):

| Nilai | `privacyPolicyUrl` | `howToUseUrl` |
|---|---|---|
| `"local"` | Konten dibaca dari `src/lib/privacy/{slug}.js` | Konten dibaca dari `src/lib/how-to-use/{slug}.js` |
| `"https://..."` | Tombol menuju kebijakan eksternal | Tombol menuju panduan eksternal |
| `null` / tidak ditulis | Halaman `/{slug}/privacy-policy` tetap ada, memakai teks kebijakan generik | Kartu panduan disembunyikan, `/{slug}/how-to-use` jadi 404 & tidak masuk sitemap |

### 5. Link (`website`, `googlePlay`, `appStore`, `download`, `chromeWebStore`, `mozillaAddOn`, `facebookGroup`, `telegramChannel`, `telegramGroup`)
Semua opsional, harus diawali `https://` atau bernilai `null`. Kartu "Unduh & Akses" hanya muncul jika minimal satu link distribusi terisi; section "Komunitas" hanya muncul jika minimal satu link komunitas terisi.

---

## ✅ Yang Diperiksa `validate.js`

- Format `slug` (kebab-case) dan sisa placeholder pada slug, nama, deskripsi, icon, serta seluruh section privacy/how-to-use → **error**.
- Kelengkapan bilingual `id`/`en` pada `name`, `description`, `shortDescription`, dan kesamaan jumlah section EN vs ID.
- Kategori & label: cocok dengan daftar standar, atau peringatan bahwa badge CSS perlu ditambahkan.
- `icon`: path `/assets/logo/{slug}.png` (nama file diingatkan agar sama dengan slug), URL https, atau `null`.
- `image`: URL https, dan peringatan jika masih memakai placeholder (`placehold.co`).
- Semua link distribusi/komunitas harus `https://`.
- `releaseDate` format `YYYY-MM-DD` dan tidak berada di masa depan.
- `pricing`, `is_access_login`, `privacyPolicyUrl`, `howToUseUrl` sesuai mode yang didukung UI.
- Field yang tidak dikenali `apps.json` → peringatan.
- Section: `title` wajib; tipe `content`/`note`/`points`/`email` benar; `youtube` harus 11 karakter ID (bukan URL) dan **dilarang** di `privacy.js` karena tidak dirender.
- Sisa blok komentar panduan template pada `privacy.js` / `how-to-use.js` → peringatan agar diganti header pendek sebelum di-commit ke repo `awanapps`.

Setelah 0 error, validator mencetak checklist merge yang sudah terisi slug aplikasi Anda.

---

## 🚀 Panduan Merge ke Repo `awanapps`

Setelah data di `.awanapps` lolos `node .awanapps/validate.js`, lakukan langkah berikut di repo `awanapps`:

1. **Tambahkan Metadata ke `apps.json`**:
   Buka `apps.json` di root repo `awanapps`, tambahkan objek dari `apps.js` ke dalam array JSON (hapus komentar — `apps.json` adalah JSON murni, bukan JS).
2. **Tambahkan Privacy Policy Lokal** *(jika `privacyPolicyUrl: "local"`)*:
   - Salin isi `privacy.js` ke file baru: `src/lib/privacy/{slug}.js`, **ganti blok komentar panduan template dengan header pendek** (`{Nama App} - Privacy Policy Content`).
   - Buka `src/lib/privacy/index.js`, import file tersebut dan daftarkan key `{slug}` pada objek `privacyData`.
3. **Tambahkan Panduan Penggunaan Lokal** *(jika `howToUseUrl: "local"`)*:
   - Salin isi `how-to-use.js` ke file baru: `src/lib/how-to-use/{slug}.js` (ganti header seperti langkah 2).
   - Buka `src/lib/how-to-use/index.js`, import file tersebut dan daftarkan key `{slug}` pada objek `howToUseData`.
4. **Salin Logo / Icon Aplikasi**:
   Simpan icon PNG aplikasi di `public/assets/logo/{slug}.png` (lewati langkah ini jika `icon` bernilai `null` atau URL eksternal).
5. **Tambahkan Badge CSS** *(hanya jika memakai kategori/label yang belum punya warna)*:
   Tambahkan `.badge-cat-{kategori}` / `.badge-val-{label}` di `src/app.css`.
6. **Uji Tampilan & Build**:
   ```bash
   npm run dev    # Cek halaman /{slug}, /{slug}/privacy-policy, /{slug}/how-to-use
   npm run build  # Pastikan bundle build berhasil tanpa error
   ```
   Sitemap (`/sitemap.xml`) dibuat otomatis dari `apps.json` — tidak ada file yang perlu diedit manual.
7. **Naikkan versi** di `package.json` repo `awanapps` lalu commit & push ke `main` (deploy Vercel berjalan otomatis).

> Folder `.awanapps` **tidak perlu ikut di-merge** ke repo `awanapps` — cukup file hasilnya. Biarkan folder ini tetap ada di repo aplikasi Anda sebagai sumber data saat metadata perlu diperbarui.

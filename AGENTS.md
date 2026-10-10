# Arahan proyek Awan Host Remote

## Tujuan aplikasi

Bangun aplikasi untuk menghubungkan dan mengontrol server melalui SSH. Pengguna dapat menambahkan host, melihat ringkasan server, menjalankan perintah, dan mengetahui alat yang sudah terpasang di server.

## Teknologi dan arsitektur

Gunakan full Rust untuk aplikasi desktop dengan prioritas penggunaan RAM yang terkendali dan antarmuka yang responsif.

- **UI desktop**: egui dengan eframe, berjalan sebagai aplikasi native.
- **Runtime asynchronous**: Tokio untuk pekerjaan jaringan dan operasi background.
- **SSH dan tunnel**: library SSH Rust yang mendukung autentikasi password/key, verifikasi host key, channel command, shell dengan PTY, dan port forwarding.
- **Terminal**: widget terminal native dengan backend emulator terminal. Evaluasi kompatibilitas egui_term atau alternatif Rust sebelum menetapkan dependency; jangan menggantinya dengan tampilan teks output biasa.
- **Database**: driver Rust untuk MySQL/MariaDB dan PostgreSQL, terhubung melalui SSH tunnel jika diperlukan.
- **Penyimpanan lokal**: SQLite untuk daftar host dan pengaturan. Kredensial disimpan menggunakan penyimpanan kredensial sistem operasi, terpisah dari data konfigurasi biasa.

Stack yang disepakati menggunakan GUI Rust native tanpa React, Electron, Tauri, atau WebView. Tentukan crate dan versi yang kompatibel saat implementasi sesuai kebutuhan; jangan menginstal dependency tanpa permintaan pengguna.

Pisahkan kode UI, pengelolaan sesi SSH, terminal, kontrol server/port, akses database, dan penyimpanan lokal agar tanggung jawab setiap bagian jelas.

## Performa dan penggunaan RAM

- Jangan menjalankan koneksi SSH, query database, atau operasi jaringan yang memblokir pada thread UI. Kirim hasil dari pekerjaan background ke UI melalui channel dengan kapasitas terbatas.
- Gunakan ulang koneksi SSH untuk channel yang sesuai. Kelola masa hidup sesi, tunnel, dan koneksi database; tutup resource ketika tidak lagi digunakan.
- Kirim output terminal secara streaming dengan batas buffer dan scrollback. Hindari menyimpan seluruh riwayat terminal tanpa batas.
- Batasi jumlah baris dan ukuran hasil query database. Gunakan pagination dan render hanya baris tabel yang terlihat untuk data besar.
- Batasi jumlah koneksi dalam pool database dan jumlah pekerjaan background yang berjalan bersamaan.
- Perbarui statistik server pada interval yang wajar dan hentikan polling yang tidak diperlukan ketika host terputus atau tampilan tidak aktif.
- Minta repaint UI saat ada perubahan atau sesuai kebutuhan animasi; hindari loop redraw terus-menerus saat aplikasi idle.
- Batasi cache dan log dalam memori. Sediakan timeout serta pembatalan untuk pekerjaan yang lama.
- Jangan menjanjikan angka penggunaan RAM sebelum pengukuran. Pengukuran performa atau pengujian hanya dijalankan jika pengguna meminta.

Terminal interaktif harus mendukung escape sequence ANSI, warna, keyboard, perubahan ukuran PTY, serta aplikasi seperti vim dan htop. Pastikan integrasi widget terminal meneruskan input dan resize ke sesi SSH.

## Alur utama pengguna

1. Saat aplikasi pertama kali dibuka, tampilkan sidebar daftar menu di sebelah kiri.
2. Jika belum ada server yang terhubung, tampilkan keadaan kosong dengan pesan **No server connected** dan tombol **Add New Host**.
3. Pengguna memilih **Add New Host**, kemudian mengisi informasi koneksi SSH server.
4. Form koneksi memuat nama host, alamat IP atau hostname, port SSH (default 22), username, dan metode autentikasi menggunakan password atau SSH key.
5. Saat koneksi berlangsung, tampilkan status proses. Jika gagal, tampilkan pesan yang mudah dipahami dan izinkan pengguna memperbaiki informasi koneksi.
6. Setelah koneksi berhasil, buka halaman ringkasan server dan tampilkan menu kontrol untuk host yang aktif.

## Tampilan dan menu

Sidebar tetap berada di sebelah kiri agar pengguna mudah berpindah halaman. Tampilkan nama host aktif dan status koneksi dengan jelas.

Menu utama yang dibutuhkan:

- **Summary**: ringkasan kondisi server yang terhubung.
- **Server Control**: kontrol layanan dan tindakan pengelolaan server.
- **Ports**: melihat port aktif dan mengelola aturan akses port pada server.
- **Databases**: memeriksa database melalui antarmuka grafis.
- **Terminal / Shell**: command line interaktif untuk menjalankan perintah pada server melalui SSH.
- **File Editor**: membuka, mengedit, dan menyimpan file teks remote melalui SSH, dengan status perubahan dan pemeriksaan konflik sebelum menyimpan.
- File Editor menyediakan Explorer folder remote agar pengguna dapat berpindah folder, memilih file, dan mengeditnya tanpa mengetik path setiap kali. Pertahankan draft ketika berpindah folder dan minta konfirmasi sebelum mengganti dokumen yang belum disimpan.
- **Applications**: katalog instalasi, daftar aplikasi/paket terpasang, serta uninstall melalui SSH.
- **Hosts**: daftar host, penambahan host baru, serta pengelolaan koneksi dan pemutusan koneksi.

Profil SSH yang berhasil terhubung dapat disimpan bersama login agar pengguna tidak perlu mengisi IP/kredensial lagi. Gunakan alamat IP yang dinormalisasi sebagai identitas unik, dengan satu profil per alamat. Simpan rahasia melalui penyimpanan kredensial OS; jangan fallback ke plaintext. Tampilkan status tersimpan, sambung ulang, serta konfirmasi penghapusan profil dan kredensial. Login gagal tidak boleh menimpa login tersimpan yang masih ada.

## Ringkasan server

Halaman Summary menampilkan informasi dari server yang sedang terhubung:

- Hostname, alamat server, sistem operasi, dan versi kernel.
- Uptime server.
- Pemakaian CPU, RAM, dan penyimpanan.
- Status koneksi SSH.

Gunakan data yang diperoleh dari server. Jika informasi tidak dapat diambil, tampilkan status tidak tersedia beserta penjelasan singkat; jangan menampilkan data contoh sebagai data nyata.

Ketentuan data nyata berlaku untuk seluruh halaman: jangan sediakan host demo, metrik statis, status layanan/firewall simulasi, atau fixture database. Bersihkan data ketika SSH terputus atau host berubah; tampilkan penyebab jika tool, izin, atau koneksi database belum tersedia.

## Kontrol server

Sediakan menu untuk melihat dan mengontrol layanan server, termasuk status layanan serta tindakan start, stop, dan restart sesuai izin akun SSH.

Tampilkan hasil tindakan dan kegagalan secara jelas. Untuk tindakan berisiko seperti reboot atau shutdown server, minta konfirmasi pengguna sebelum menjalankan perintah.

## Pengelolaan port

- Tampilkan daftar port yang sedang listening, protokol TCP/UDP, alamat bind, serta proses atau layanan pemiliknya jika informasi tersedia.
- Sediakan pencarian dan filter berdasarkan nomor port, protokol, atau layanan.
- Tampilkan aturan firewall yang berlaku dan izinkan pengguna menambah atau menghapus aturan untuk membuka atau menutup akses port sesuai izin akun SSH.
- Bedakan port yang sedang listening dengan port yang diizinkan firewall; aturan firewall tidak otomatis menjalankan atau menghentikan layanan.
- Deteksi firewall yang tersedia pada server dan tampilkan penjelasan jika pengelolaan tidak didukung atau membutuhkan izin tambahan.
- Minta konfirmasi sebelum mengubah aturan firewall. Beri peringatan khusus jika perubahan dapat memutus akses SSH yang sedang digunakan.
- Tampilkan hasil perubahan dan muat ulang status port serta aturan firewall setelah tindakan selesai.

## Pemeriksaan database melalui GUI

- Sediakan form koneksi database yang memuat jenis database, alamat host, port, username, password, dan nama database jika diperlukan.
- Utamakan koneksi melalui SSH tunnel host aktif agar pengguna dapat mengakses database internal tanpa membuka port database ke publik.
- Dukungan awal mencakup MySQL/MariaDB dan PostgreSQL. Tampilkan jenis database yang didukung dengan jelas.
- Tampilkan status koneksi, daftar database, schema jika tersedia, tabel, serta struktur kolom, tipe data, primary key, dan index.
- Izinkan pengguna melihat isi tabel melalui GUI dengan pagination, pencarian, filter, dan pengurutan agar tabel besar tidak dimuat sekaligus.
- Sediakan editor query untuk pemeriksaan data dan tampilkan hasil dalam tabel beserta pesan error yang mudah dipahami.
- Pemeriksaan database menggunakan mode baca saja secara default. Perubahan atau penghapusan data tidak termasuk cakupan fitur ini kecuali diminta pengguna.
- Jangan menampilkan atau mencatat password database dalam log. Simpan kredensial secara aman jika pengguna memilih menyimpannya.
- Tampilkan informasi jika akun database tidak memiliki izin untuk membaca objek tertentu.

## Terminal / Shell

- Terminal menjalankan perintah pada host aktif melalui koneksi SSH.
- Tampilkan output, error, dan status koneksi secara jelas.
- Dukung input keyboard dan sesi shell interaktif.
- Saat koneksi terputus, beri tahu pengguna dan sediakan tindakan untuk menghubungkan ulang.

## Status alat yang terpasang

Periksa ketersediaan alat pada server, dimulai dari Git, Docker, dan Docker Compose. Daftar alat dapat diperluas sesuai kebutuhan pengguna.

Untuk setiap alat, tampilkan:

- Nama alat.
- Status **Installed**, **Not Installed**, atau **Unknown** jika pemeriksaan gagal.
- Versi alat jika tersedia.
- Untuk Docker, tampilkan juga status daemon jika dapat diperiksa.

Pemeriksaan status tidak boleh otomatis menginstal atau memperbarui alat. Instalasi hanya dilakukan atas permintaan pengguna.

Sediakan katalog instalasi mudah untuk Nginx, Docker, Git, dan alat server lainnya, serta daftar paket terpasang yang dapat dicari. Install dan uninstall harus menampilkan target host, konfirmasi, log proses, dan hasil yang jelas; muat ulang status sesudah selesai. Gunakan package manager server yang didukung dan jelaskan jika paket tidak tersedia. Sebelum penghapusan, tampilkan rencana package manager agar pengguna dapat meninjau paket lain yang ikut terdampak. Jangan otomatis menghapus data aplikasi.

## Keamanan koneksi

- Jangan menampilkan password atau isi private key di log maupun pesan error.
- Jangan menyimpan kredensial dalam bentuk plaintext.
- Verifikasi identitas host SSH. Minta keputusan pengguna untuk host baru atau perubahan host key sebelum melanjutkan koneksi.
- Jalankan tindakan sesuai hak akses akun SSH dan tampilkan informasi jika izin tidak mencukupi.

## Preferensi kerja pengembangan

- Baca hanya kode dan instruksi yang relevan dengan permintaan.
- Analisis bagian yang perlu diubah, jelaskan rencana secara singkat, lalu selesaikan perubahan yang diminta.
- Jangan memperluas cakupan ke refactor atau fitur lain yang tidak diminta. Arahan produk dalam file ini bukan perintah untuk mengimplementasikan seluruh fitur sekaligus.
- Pengguna menangani pengujian sendiri, kecuali secara eksplisit meminta bantuan pengujian.
- Jangan menjalankan tes, lint, analyzer, emulator, aplikasi, server pengembangan, atau build/APK kecuali diminta secara eksplisit.
- Jangan menginstal atau memperbarui dependency maupun alat tanpa diminta. Jika diperlukan, jelaskan dependency dan perintah yang dapat dijalankan pengguna.
- Jangan menjalankan subagent atau pekerjaan paralel antaragen kecuali pengguna meminta.
- Jika pengguna hanya bertanya atau meminta penjelasan, jawab tanpa mengubah kode.
- Berikan update singkat tentang pekerjaan, hasil, atau kendala.
- Setelah perubahan selesai, ringkas perubahan dan langkah pengujian yang perlu dilakukan pengguna. Nyatakan bahwa kode belum diuji jika tidak ada pengujian yang dijalankan.
- Instruksi eksplisit pengguna dalam sesi dapat memberikan pengecualian; kerjakan hanya pengecualian yang diminta.

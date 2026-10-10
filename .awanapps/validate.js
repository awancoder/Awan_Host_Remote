#!/usr/bin/env node
/**
 * ============================================================================
 * .awanapps / validate.js — Skrip Validasi Otomatis Template Awan Apps
 * ============================================================================
 *
 * Jalankan skrip ini untuk memastikan seluruh data di `.awanapps` sudah valid,
 * tidak ada sisa placeholder template, dan siap di-merge ke repo `awanapps`:
 *
 *   node .awanapps/validate.js
 *
 * Zero dependency — hanya butuh Node.js 18+.
 * Folder ini punya `package.json` dengan `"type": "module"` supaya skrip tetap
 * jalan walau project induknya bukan project Node/ESM (Flutter, Android, dll).
 *
 * Exit code: 0 = siap merge, 1 = masih ada error.
 * ============================================================================
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Kategori yang SUDAH punya class badge warna (.badge-cat-*) di src/app.css awanapps
const BADGED_CATEGORIES = [
  'Utilities',
  'Productivity',
  'Developer Tool',
  'Health',
  'Accounting',
  'Media Social',
  'Marketing & Sales',
  'Finance',
  'Game',
  'Design'
];

// Kategori yang sudah dipakai di apps.json tapi BELUM punya badge warna (tampil netral)
const CATALOG_CATEGORIES_NO_BADGE = [
  'Health & Fitness',
  'Video Editor',
  'Tools',
  'Education',
  'Travel & Local'
];

// Label platform resmi (semuanya sudah punya class .badge-val-* di src/app.css)
const KNOWN_LABELS = ['Web App', 'Desktop', 'Mobile', 'Extension', 'Game'];

// Field yang dikenali oleh apps.json di repo awanapps
const KNOWN_APP_FIELDS = [
  'slug', 'name', 'category', 'label', 'description', 'shortDescription',
  'icon', 'image', 'website', 'googlePlay', 'appStore', 'download',
  'mozillaAddOn', 'chromeWebStore', 'facebookGroup', 'telegramChannel',
  'telegramGroup', 'pricing', 'is_access_login', 'releaseDate',
  'privacyPolicyUrl', 'howToUseUrl'
];

const LINK_FIELDS = [
  'website', 'googlePlay', 'appStore', 'download', 'mozillaAddOn',
  'chromeWebStore', 'facebookGroup', 'telegramChannel', 'telegramGroup'
];

// Potongan teks bawaan template yang WAJIB diganti sebelum merge
const PLACEHOLDER_SNIPPETS = [
  'nama-aplikasi',
  'Nama Aplikasi',
  'Application Name',
  'Jelaskan fungsi utama',
  'Describe the main purpose',
  'Ringkasan 1 kalimat',
  'A concise 1-sentence summary',
  'PERMISSION_NAME',
  'LOCAL_STORAGE / STORAGE',
  'NAMA_IZIN',
  'PENYIMPANAN_LOKAL',
  'DATA_TYPE',
  'JENIS_DATA',
  'APP_PREFERENCES',
  'PREFERENSI_APLIKASI',
  'Step 1: Click the primary action button',
  'Langkah 1: Klik tombol aksi utama',
  'dQw4w9WgXcQ'
];

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const TODAY = new Date().toISOString().split('T')[0];

let totalErrors = 0;
let totalWarnings = 0;

function logPass(msg) {
  console.log(`  \x1b[32m✔\x1b[0m ${msg}`);
}

function logWarn(msg) {
  totalWarnings++;
  console.log(`  \x1b[33m▲\x1b[0m \x1b[33m[PERINGATAN]\x1b[0m ${msg}`);
}

function logFail(msg) {
  totalErrors++;
  console.log(`  \x1b[31m✖\x1b[0m \x1b[31m[ERROR]\x1b[0m ${msg}`);
}

function readRaw(fileName) {
  const filePath = path.join(__dirname, fileName);
  if (!fs.existsSync(filePath)) return null;
  return fs.readFileSync(filePath, 'utf8');
}

async function loadModule(fileName) {
  const filePath = path.join(__dirname, fileName);
  if (!fs.existsSync(filePath)) {
    logFail(`File tidak ditemukan: ${fileName}`);
    return null;
  }
  try {
    const fileUrl = pathToFileURL(filePath).href;
    const mod = await import(fileUrl);
    return mod.default || mod;
  } catch (err) {
    logFail(`Gagal mengimpor ${fileName}: ${err.message}`);
    return null;
  }
}

/** Cari sisa teks placeholder template pada sebuah string. */
function findPlaceholder(text) {
  if (typeof text !== 'string') return null;
  return PLACEHOLDER_SNIPPETS.find(snippet => text.includes(snippet)) || null;
}

function checkOptionalUrl(app, field) {
  const val = app[field];
  if (val === null || val === undefined || val === '') return;
  if (typeof val !== 'string') {
    logFail(`Field \`${field}\` harus berupa URL string atau null.`);
    return;
  }
  if (!/^https:\/\//.test(val)) {
    logFail(`Field \`${field}\` harus diawali "https://" (nilai sekarang: "${val}").`);
    return;
  }
  logPass(`${field}: ${val}`);
}

function validateApp(app, index, total) {
  const prefix = total > 1 ? `apps.js[${index}] ` : '';

  // 1. Slug
  if (!app.slug || typeof app.slug !== 'string') {
    logFail(`${prefix}Field \`slug\` wajib diisi sebagai string.`);
  } else if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(app.slug)) {
    logFail(`${prefix}Slug "${app.slug}" tidak valid. Harus kebab-case (huruf kecil, angka, strip).`);
  } else if (app.slug === 'nama-aplikasi' || app.slug === 'nama-app') {
    logFail(`${prefix}Slug masih placeholder default "${app.slug}". Ganti dengan slug asli aplikasi.`);
  } else {
    logPass(`Slug valid: "${app.slug}"`);
  }

  // 2. Name
  if (!app.name) {
    logFail(`${prefix}Field \`name\` wajib diisi.`);
  } else if (typeof app.name === 'object') {
    if (!app.name.id || !app.name.en) {
      logFail(`${prefix}Field \`name\` berupa objek harus memiliki properti \`id\` dan \`en\`.`);
    } else if (findPlaceholder(app.name.id) || findPlaceholder(app.name.en)) {
      logFail(`${prefix}Field \`name\` masih memakai nama placeholder template.`);
    } else {
      logPass(`Nama aplikasi: ID="${app.name.id}", EN="${app.name.en}"`);
    }
  } else if (typeof app.name === 'string') {
    logWarn(`${prefix}Field \`name\` berupa string. Sebaiknya pakai objek { id, en } agar konsisten dengan katalog.`);
  } else {
    logFail(`${prefix}Field \`name\` harus berupa objek { id, en }.`);
  }

  // 3. Category
  if (!app.category || typeof app.category !== 'string') {
    logFail(`${prefix}Field \`category\` wajib diisi (string tunggal).`);
  } else if (BADGED_CATEGORIES.includes(app.category)) {
    logPass(`Kategori valid (punya badge warna): "${app.category}"`);
  } else if (CATALOG_CATEGORIES_NO_BADGE.includes(app.category)) {
    logWarn(`Kategori "${app.category}" dipakai di katalog tapi BELUM punya class \`.badge-cat-${kebab(app.category)}\` di src/app.css — badge akan tampil netral.`);
  } else {
    logWarn(`Kategori "${app.category}" baru (belum ada di katalog). Saat merge, tambahkan class \`.badge-cat-${kebab(app.category)}\` di src/app.css.`);
  }

  // 4. Label
  if (!app.label) {
    logFail(`${prefix}Field \`label\` wajib diisi (array string).`);
  } else {
    const labels = Array.isArray(app.label) ? app.label : [app.label];
    if (!Array.isArray(app.label)) {
      logWarn(`${prefix}Field \`label\` sebaiknya berupa array, misal ["Mobile"].`);
    }
    if (labels.length === 0) {
      logFail(`${prefix}Array \`label\` tidak boleh kosong.`);
    } else {
      labels.forEach(lbl => {
        if (KNOWN_LABELS.includes(lbl)) {
          logPass(`Label valid: "${lbl}"`);
        } else {
          logWarn(`Label "${lbl}" di luar daftar standar. Saat merge, tambahkan class \`.badge-val-${kebab(String(lbl))}\` di src/app.css.`);
        }
      });
    }
  }

  // 5. Description & Short Description
  ['description', 'shortDescription'].forEach(field => {
    const val = app[field];
    if (!val || typeof val !== 'object' || !val.id || !val.en) {
      logFail(`${prefix}Field \`${field}\` wajib memiliki properti \`id\` dan \`en\`.`);
      return;
    }
    const ph = findPlaceholder(val.id) || findPlaceholder(val.en);
    if (ph) {
      logFail(`${prefix}Field \`${field}\` masih berisi teks placeholder template ("${ph}"). Tulis deskripsi asli aplikasi.`);
      return;
    }
    if (field === 'shortDescription' && (val.id.length > 160 || val.en.length > 160)) {
      logWarn(`${prefix}\`shortDescription\` cukup panjang (>160 karakter). Idealnya 1 kalimat padat untuk card listing.`);
    }
    logPass(`${field} (ID & EN) tersedia.`);
  });

  // 6. Icon
  if (app.icon === null) {
    logWarn('Icon bernilai `null` — UI akan menampilkan kotak fallback huruf inisial. Sebaiknya siapkan PNG 512x512 transparan.');
  } else if (!app.icon || typeof app.icon !== 'string') {
    logFail(`${prefix}Field \`icon\` harus string path ("/assets/logo/{slug}.png"), URL https, atau null.`);
  } else if (app.icon.startsWith('/assets/logo/')) {
    const base = app.icon.slice('/assets/logo/'.length);
    if (findPlaceholder(base)) {
      logFail(`${prefix}Path icon masih placeholder: "${app.icon}".`);
    } else if (app.slug && base !== `${app.slug}.png`) {
      logWarn(`Nama file icon "${base}" tidak sama dengan slug. Disarankan "${app.slug}.png" agar mudah dilacak.`);
    } else {
      logPass(`Path icon: "${app.icon}" (salin asset PNG ke public${app.icon} di repo awanapps).`);
    }
  } else if (/^https:\/\//.test(app.icon)) {
    logPass(`Icon memakai URL eksternal: ${app.icon}`);
  } else {
    logFail(`${prefix}Field \`icon\` "${app.icon}" tidak valid. Pakai "/assets/logo/{slug}.png", URL https, atau null.`);
  }

  // 7. Image (banner / og:image)
  if (app.image !== null && app.image !== undefined && app.image !== '') {
    if (typeof app.image !== 'string' || !/^https:\/\//.test(app.image)) {
      logFail(`${prefix}Field \`image\` harus URL https atau null.`);
    } else if (app.image.includes('placehold.co') || app.image.includes('placeholder')) {
      logWarn('Field `image` masih memakai URL placeholder. Ganti dengan banner asli atau set null.');
    } else {
      logPass(`Image banner: ${app.image}`);
    }
  }

  // 8. Link distribusi & komunitas
  LINK_FIELDS.forEach(field => checkOptionalUrl(app, field));
  const hasDistribution = ['website', 'googlePlay', 'appStore', 'download', 'mozillaAddOn', 'chromeWebStore']
    .some(f => typeof app[f] === 'string' && app[f]);
  if (!hasDistribution) {
    logWarn('Belum ada satu pun link distribusi (website/googlePlay/appStore/download/chromeWebStore/mozillaAddOn) — kartu "Unduh & Akses" tidak akan muncul di halaman detail.');
  }

  // 9. Release Date
  if (app.releaseDate !== null && app.releaseDate !== undefined) {
    if (typeof app.releaseDate !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(app.releaseDate)) {
      logFail(`${prefix}Format releaseDate "${app.releaseDate}" tidak valid. Harus YYYY-MM-DD atau null.`);
    } else if (Number.isNaN(Date.parse(app.releaseDate))) {
      logFail(`${prefix}releaseDate "${app.releaseDate}" bukan tanggal yang valid.`);
    } else if (app.releaseDate > TODAY) {
      logWarn(`releaseDate "${app.releaseDate}" berada di masa depan (hari ini ${TODAY}). Pastikan ini memang tanggal rilis terjadwal.`);
    } else {
      logPass(`Tanggal rilis: ${app.releaseDate}`);
    }
  } else {
    logWarn('Field `releaseDate` kosong — baris "Rilis" di halaman detail akan tampil tanpa tanggal.');
  }

  // 10. pricing & is_access_login
  const validPricingModes = ['free', 'ads', 'paid', undefined, null];
  if (!validPricingModes.includes(app.pricing)) {
    logFail(`${prefix}Field \`pricing\` "${app.pricing}" tidak valid. Harus "free", "ads", atau "paid".`);
  } else {
    const pricingVal = app.pricing || 'free';
    const pricingLabels = { free: 'Gratis', ads: 'Berisi Iklan', paid: 'Berbayar' };
    logPass(`pricing: "${pricingVal}" (${pricingLabels[pricingVal]})`);
  }

  if (app.is_access_login !== undefined && typeof app.is_access_login !== 'boolean') {
    logFail(`${prefix}Field \`is_access_login\` harus berupa boolean (true/false).`);
  } else {
    logPass(`is_access_login: ${app.is_access_login ?? false} (${app.is_access_login ? 'Perlu Login' : 'Tanpa Login'})`);
  }

  // 11. Privacy & How to Use URL Modes
  [['privacyPolicyUrl', 'Privacy Policy'], ['howToUseUrl', 'How To Use']].forEach(([field, labelName]) => {
    const val = app[field];
    if (val === 'local') {
      logPass(`${labelName} mode: "local" (isi ${field === 'privacyPolicyUrl' ? 'privacy.js' : 'how-to-use.js'} wajib lengkap).`);
    } else if (typeof val === 'string' && /^https:\/\//.test(val)) {
      logPass(`${labelName} mode eksternal: ${val}`);
    } else if (val === null || val === undefined) {
      const effect = field === 'privacyPolicyUrl'
        ? 'halaman /{slug}/privacy-policy memakai teks kebijakan generik'
        : 'kartu panduan disembunyikan & /{slug}/how-to-use jadi 404';
      logPass(`${labelName} mode: kosong (${effect}).`);
    } else {
      logFail(`${prefix}Nilai \`${field}\` "${val}" tidak valid. Harus "local", URL "https://...", atau null.`);
    }
  });

  // 12. Field tak dikenal
  Object.keys(app).forEach(key => {
    if (!KNOWN_APP_FIELDS.includes(key)) {
      logWarn(`Field \`${key}\` tidak dikenali di apps.json repo awanapps dan akan diabaikan UI.`);
    }
  });
}

function kebab(str) {
  return String(str).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
}

async function validateApps(appsData) {
  console.log('\n\x1b[36m▶ Memeriksa apps.js...\x1b[0m');
  if (!appsData) return null;

  const list = Array.isArray(appsData) ? appsData : [appsData];
  if (list.length === 0 || typeof list[0] !== 'object' || list[0] === null) {
    logFail('apps.js harus mengekspor object atau array berisi object aplikasi.');
    return null;
  }

  if (list.length > 1) {
    logWarn(`apps.js berisi ${list.length} aplikasi. Biasanya satu kit = satu aplikasi; pastikan ini memang disengaja.`);
    const slugs = list.map(a => a && a.slug).filter(Boolean);
    const dupes = slugs.filter((s, i) => slugs.indexOf(s) !== i);
    if (dupes.length) logFail(`Slug duplikat di apps.js: ${[...new Set(dupes)].join(', ')}`);
  }

  list.forEach((app, i) => validateApp(app, i, list.length));
  return list;
}

function validateSectionDocument(doc, docName) {
  console.log(`\n\x1b[36m▶ Memeriksa ${docName}...\x1b[0m`);
  if (!doc || typeof doc !== 'object') {
    logFail(`${docName} harus mengekspor objek { en: {...}, id: {...} }.`);
    return;
  }

  const isPrivacy = docName.startsWith('privacy');

  // Sisa blok komentar panduan template — harus dihapus sebelum merge
  const raw = readRaw(docName);
  if (raw && raw.includes('.awanapps / ' + docName + ' — Template')) {
    logWarn(`${docName} masih memuat blok komentar panduan template. Ganti dengan header pendek saat menyalin ke repo awanapps.`);
  }

  const counts = {};
  ['en', 'id'].forEach(lang => {
    const langData = doc[lang];
    if (!langData) {
      logFail(`${docName} tidak memiliki entri bahasa "${lang}".`);
      return;
    }

    if (!langData.lastUpdated || typeof langData.lastUpdated !== 'string') {
      logWarn(`${docName}[${lang}] tidak memiliki field lastUpdated.`);
    }

    if (!Array.isArray(langData.sections) || langData.sections.length === 0) {
      logFail(`${docName}[${lang}].sections harus berupa array dan tidak boleh kosong.`);
      return;
    }

    counts[lang] = langData.sections.length;
    logPass(`${docName}[${lang}] memiliki ${langData.sections.length} section.`);

    langData.sections.forEach((sec, idx) => {
      const at = `${docName}[${lang}] section #${idx + 1}`;

      if (!sec || typeof sec !== 'object') {
        logFail(`${at} bukan objek section yang valid.`);
        return;
      }
      if (!sec.title || typeof sec.title !== 'string') {
        logFail(`${at} tidak memiliki judul (\`title\`).`);
      }
      if (!sec.content && !sec.points && !sec.note && !sec.email && !sec.youtube) {
        logWarn(`${at} ("${sec.title}") hanya berisi judul tanpa konten apa pun.`);
      }
      ['content', 'note'].forEach(f => {
        if (sec[f] !== undefined && typeof sec[f] !== 'string') {
          logFail(`${at} field \`${f}\` harus berupa string.`);
        }
      });
      if (sec.points !== undefined) {
        if (!Array.isArray(sec.points)) {
          logFail(`${at} field \`points\` harus berupa array string.`);
        } else if (sec.points.length === 0) {
          logWarn(`${at} field \`points\` kosong — hapus saja field-nya.`);
        } else if (sec.points.some(p => typeof p !== 'string' || !p.trim())) {
          logFail(`${at} field \`points\` harus berisi string yang tidak kosong.`);
        }
      }
      if (sec.email !== undefined && !EMAIL_RE.test(String(sec.email))) {
        logFail(`${at} field \`email\` "${sec.email}" bukan alamat email yang valid.`);
      }

      // Sisa placeholder template
      const texts = [sec.title, sec.content, sec.note, ...(Array.isArray(sec.points) ? sec.points : [])];
      const ph = texts.map(findPlaceholder).find(Boolean);
      if (ph) {
        logFail(`${at} masih berisi teks placeholder template ("${ph}"). Ganti dengan konten asli aplikasi.`);
      }

      // youtube
      if (sec.youtube !== undefined) {
        if (isPrivacy) {
          logFail(`${at} memakai field \`youtube\`, padahal halaman privacy policy tidak merender video. Hapus field ini.`);
        } else if (typeof sec.youtube !== 'string' || sec.youtube.trim() === '') {
          logFail(`${at} field \`youtube\` kosong. Isi 11 karakter ID video, atau HAPUS field-nya.`);
        } else if (/https?:\/\//.test(sec.youtube) || sec.youtube.includes('/')) {
          logFail(`${at} memakai URL YouTube lengkap. Masukkan HANYA 11 karakter ID video (misal: "dQw4w9WgXcQ").`);
        } else if (sec.youtube.length !== 11) {
          logWarn(`${at} ID youtube "${sec.youtube}" tidak berukuran standar 11 karakter.`);
        }
      }
    });
  });

  if (counts.en && counts.id && counts.en !== counts.id) {
    logWarn(`${docName} jumlah section berbeda antara EN (${counts.en}) dan ID (${counts.id}). Kedua bahasa sebaiknya saling terjemahan.`);
  }
}

function printMergeChecklist(apps) {
  const app = apps && apps[0];
  if (!app) return;
  console.log('\n\x1b[36m▶ Checklist merge ke repo awanapps:\x1b[0m');
  console.log(`  1. Tambahkan objek dari apps.js ke array di \`apps.json\`.`);
  if (typeof app.icon === 'string' && app.icon.startsWith('/assets/logo/')) {
    console.log(`  2. Salin icon PNG ke \`public${app.icon}\`.`);
  } else {
    console.log(`  2. (Icon eksternal / null — tidak ada asset yang perlu disalin.)`);
  }
  if (app.privacyPolicyUrl === 'local') {
    console.log(`  3. Salin privacy.js → \`src/lib/privacy/${app.slug}.js\` + daftarkan di \`src/lib/privacy/index.js\`.`);
  }
  if (app.howToUseUrl === 'local') {
    console.log(`  4. Salin how-to-use.js → \`src/lib/how-to-use/${app.slug}.js\` + daftarkan di \`src/lib/how-to-use/index.js\`.`);
  }
  console.log(`  5. Jalankan \`npm run dev\` lalu cek /${app.slug}, /${app.slug}/privacy-policy${app.howToUseUrl ? `, /${app.slug}/how-to-use` : ''}.`);
  console.log(`  6. Jalankan \`npm run build\` dan naikkan versi di package.json repo awanapps.`);
}

async function run() {
  console.log('====================================================');
  console.log('🔍 VALIDASI TEMPLATE .AWANAPPS');
  console.log('====================================================');

  const appsData = await loadModule('apps.js');
  const apps = await validateApps(appsData);

  if (apps) {
    if (apps.some(a => a.privacyPolicyUrl === 'local')) {
      const privacyData = await loadModule('privacy.js');
      if (privacyData) validateSectionDocument(privacyData, 'privacy.js');
    }

    if (apps.some(a => a.howToUseUrl === 'local')) {
      const howToUseData = await loadModule('how-to-use.js');
      if (howToUseData) validateSectionDocument(howToUseData, 'how-to-use.js');
    }
  }

  console.log('\n====================================================');
  console.log('📊 HASIL VALIDASI');
  console.log('====================================================');
  console.log(`Total Error     : ${totalErrors > 0 ? `\x1b[31m${totalErrors}\x1b[0m` : '\x1b[32m0\x1b[0m'}`);
  console.log(`Total Peringatan: ${totalWarnings > 0 ? `\x1b[33m${totalWarnings}\x1b[0m` : '\x1b[32m0\x1b[0m'}`);

  if (totalErrors === 0) {
    printMergeChecklist(apps);
    console.log('\n\x1b[32m🎉 SEMUA PEMERIKSAAN BERHASIL! Data siap di-merge ke repo awanapps.\x1b[0m\n');
    process.exit(0);
  } else {
    console.log('\n\x1b[31m❌ ADA ERROR YANG HARUS DIPERBAIKI sebelum melakukan merge ke awanapps.\x1b[0m\n');
    process.exit(1);
  }
}

run();

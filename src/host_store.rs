//! Windows Credential Manager stores both profile metadata and its login secret.
//! No plaintext credentials, extra runtime, or dependencies are required.
use anyhow::{Context, Result, bail};
use zeroize::Zeroizing;
use crate::model::{Host, HostForm};

pub fn address_key(address: &str) -> String {
    let address = address.trim();
    let address = address.strip_prefix('[').and_then(|a| a.strip_suffix(']')).unwrap_or(address);
    match address.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.to_string(),
        Ok(std::net::IpAddr::V6(ip)) => ip.to_ipv4_mapped().map(|ip| ip.to_string()).unwrap_or_else(|| ip.to_string()),
        Err(_) => address.trim_end_matches('.').to_lowercase(),
    }
}

pub struct Login {
    pub host: Host,
    pub key_path: String,
    pub secret: Zeroizing<String>,
}

#[cfg(windows)]
const PREFIX: &str = "AwanHostRemote/ssh/v1/";
#[cfg(windows)]
const MAX_BLOB: usize = 2560;

#[cfg(windows)]
fn encode(host: &Host, key_path: &str, secret: &str) -> Result<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(b"AHSSH01\n".to_vec());
    let port = host.port.to_string();
    for field in [host.name.as_str(), host.address.as_str(), &port, host.user.as_str(),
        if host.key_auth { "key" } else { "password" }, key_path, secret] {
        if field.len() > MAX_BLOB || field.contains('\0') { bail!("Profil atau kredensial terlalu panjang / berisi NUL."); }
        bytes.extend_from_slice(&(field.len() as u32).to_le_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    if bytes.len() > MAX_BLOB { bail!("Profil dan login melebihi batas Credential Manager (2.560 byte). SSH tetap dapat dipakai tanpa menyimpan login."); }
    Ok(bytes)
}

#[cfg(windows)]
fn decode(bytes: &[u8]) -> Result<Login> {
    if bytes.len() > MAX_BLOB || !bytes.starts_with(b"AHSSH01\n") { bail!("Format profil tersimpan tidak dikenal."); }
    let mut offset = 8;
    let mut fields = Zeroizing::new(Vec::<String>::new());
    for _ in 0..7 {
        let length_bytes: [u8; 4] = bytes.get(offset..offset+4).context("Profil tidak lengkap")?.try_into()?;
        let length = u32::from_le_bytes(length_bytes) as usize;
        offset += 4;
        if length > MAX_BLOB { bail!("Ukuran kolom profil tidak valid."); }
        fields.push(std::str::from_utf8(bytes.get(offset..offset+length).context("Profil tidak lengkap")?)?.into());
        offset += length;
    }
    if offset != bytes.len() || !["key", "password"].contains(&fields[4].as_str()) { bail!("Profil tidak valid."); }
    let mut host = HostForm {
        name: fields[0].clone(), address: fields[1].clone(), port: fields[2].clone(), user: fields[3].clone(),
        key_auth: fields[4] == "key",
    }.host().map_err(anyhow::Error::msg)?;
    host.saved = true;
    Ok(Login { host, key_path: fields[5].clone(), secret: Zeroizing::new(fields[6].clone()) })
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{ffi::c_void, ptr};
    use zeroize::Zeroize;

    // Layout and signatures match wincred.h. These buffers are owned by Windows
    // until CredFree; the only unsafe code is this small FFI boundary.
    #[repr(C)]
    struct Credential {
        flags: u32, kind: u32, target: *mut u16, comment: *mut u16,
        written: [u32; 2], blob_size: u32, blob: *mut u8,
        persist: u32, attribute_count: u32, attributes: *mut c_void,
        alias: *mut u16, username: *mut u16,
    }
    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn CredWriteW(credential: *const Credential, flags: u32) -> i32;
        fn CredReadW(target: *const u16, kind: u32, flags: u32, credential: *mut *mut Credential) -> i32;
        fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
        fn CredEnumerateW(filter: *const u16, flags: u32, count: *mut u32, credentials: *mut *mut *mut Credential) -> i32;
        fn CredFree(buffer: *mut c_void);
    }

    fn wide(value: &str) -> Result<Vec<u16>> {
        if value.contains('\0') { bail!("Nama profil berisi NUL."); }
        Ok(value.encode_utf16().chain(std::iter::once(0)).collect())
    }
    fn target(address: &str) -> String { format!("{PREFIX}{}", address_key(address)) }

    unsafe fn wipe(credential: *mut Credential) {
        if !credential.is_null() {
            let value = unsafe { &mut *credential };
            if !value.blob.is_null() && value.blob_size as usize <= MAX_BLOB {
                unsafe { std::slice::from_raw_parts_mut(value.blob, value.blob_size as usize) }.zeroize();
            }
        }
    }
    struct One(*mut Credential);
    impl Drop for One {
        fn drop(&mut self) { unsafe { wipe(self.0); CredFree(self.0.cast()); } }
    }
    struct Many { pointer: *mut *mut Credential, count: usize }
    impl Drop for Many {
        fn drop(&mut self) {
            unsafe {
                if !self.pointer.is_null() {
                    for index in 0..self.count { wipe(*self.pointer.add(index)); }
                    CredFree(self.pointer.cast());
                }
            }
        }
    }
    unsafe fn read_entry(pointer: *mut Credential) -> Result<Login> {
        if pointer.is_null() { bail!("Credential Manager mengembalikan entri kosong."); }
        let entry = unsafe { &*pointer };
        if entry.kind != 1 || entry.blob.is_null() || entry.blob_size as usize > MAX_BLOB || entry.target.is_null() { bail!("Entri credential tidak valid."); }
        let login = decode(unsafe { std::slice::from_raw_parts(entry.blob, entry.blob_size as usize) })?;
        let mut length = 0;
        while length < 32767 && unsafe { *entry.target.add(length) } != 0 { length += 1; }
        if length == 32767 { bail!("Target credential terlalu panjang."); }
        let name = String::from_utf16(unsafe { std::slice::from_raw_parts(entry.target, length) })?;
        if name.to_lowercase() != target(&login.host.address).to_lowercase() { bail!("IP profil tidak cocok dengan target credential."); }
        Ok(login)
    }

    pub fn save(host: &Host, key_path: &str, secret: &str) -> Result<()> {
        let mut bytes = encode(host, key_path, secret)?;
        let mut name = wide(&target(&host.address))?;
        let entry = Credential {
            flags: 0, kind: 1, target: name.as_mut_ptr(), comment: ptr::null_mut(), written: [0; 2],
            blob_size: bytes.len() as u32, blob: bytes.as_mut_ptr(), persist: 2,
            attribute_count: 0, attributes: ptr::null_mut(), alias: ptr::null_mut(), username: ptr::null_mut(),
        };
        if unsafe { CredWriteW(&entry, 0) } == 0 {
            return Err(std::io::Error::last_os_error()).context("Login berhasil, tetapi profil gagal disimpan ke Windows Credential Manager");
        }
        Ok(())
    }
    pub fn read(address: &str) -> Result<Login> {
        let name = wide(&target(address))?;
        let mut pointer = ptr::null_mut();
        if unsafe { CredReadW(name.as_ptr(), 1, 0, &mut pointer) } == 0 {
            return Err(std::io::Error::last_os_error()).context("Login tersimpan tidak dapat dibaca; gunakan login manual");
        }
        let guard = One(pointer);
        unsafe { read_entry(guard.0) }
    }
    pub fn list() -> Result<(Vec<Host>, usize)> {
        let filter = wide(&format!("{PREFIX}*"))?;
        let mut count = 0;
        let mut pointer = ptr::null_mut();
        if unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut pointer) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(1168) { return Ok((Vec::new(), 0)); }
            return Err(error).context("Daftar SSH tersimpan tidak dapat dibaca");
        }
        let guard = Many { pointer, count: count as usize };
        if guard.count > 1000 || (guard.count > 0 && guard.pointer.is_null()) { bail!("Daftar credential tidak valid atau melebihi 1.000 profil."); }
        let mut hosts = Vec::new();
        let mut skipped = 0;
        for index in 0..guard.count {
            match unsafe { read_entry(*guard.pointer.add(index)) } {
                Ok(login) => hosts.push(login.host),
                Err(_) => skipped += 1,
            }
        }
        hosts.sort_by(|a, b| a.address.cmp(&b.address));
        hosts.dedup_by(|a, b| a.address == b.address);
        Ok((hosts, skipped))
    }
    pub fn delete(address: &str) -> Result<()> {
        let name = wide(&target(address))?;
        if unsafe { CredDeleteW(name.as_ptr(), 1, 0) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(1168) { return Err(error).context("Gagal menghapus login tersimpan"); }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn list() -> Result<(Vec<Host>, usize)> { bail!("Penyimpanan login saat ini tersedia pada Windows. Gunakan login manual pada platform ini."); }
    pub fn read(_: &str) -> Result<Login> { bail!("Penyimpanan login memerlukan Windows Credential Manager."); }
    pub fn save(_: &Host, _: &str, _: &str) -> Result<()> { bail!("Penyimpanan login memerlukan Windows Credential Manager."); }
    pub fn delete(_: &str) -> Result<()> { bail!("Penyimpanan login memerlukan Windows Credential Manager."); }
}
pub use platform::{read, save};

#[derive(Clone)]
pub enum Event {
    Listed(Result<(Vec<Host>, usize), String>),
    Deleted(String, Result<(), String>),
}
#[derive(Debug)]
enum Job { Delete(String) }
pub struct Store {
    jobs: tokio::sync::mpsc::Sender<Job>,
    pub busy: bool,
}
impl Store {
    /// Spawns the profile-loading worker thread and returns the store handle
    /// together with its event receiver (moved out immediately, since the
    /// caller owns a GPUI `Context` and spawns the task that drains it).
    pub fn new() -> (Self, tokio::sync::mpsc::Receiver<Event>) {
        let (jobs, mut requests) = tokio::sync::mpsc::channel(1);
        let (sender, events) = tokio::sync::mpsc::channel(2);
        let fallback = sender.clone();
        let started = std::thread::Builder::new().name("awan-profiles".into()).spawn(move || {
            let result = platform::list().map_err(|error| format!("{error:#}"));
            if sender.blocking_send(Event::Listed(result)).is_err() { return; }
            while let Some(Job::Delete(address)) = requests.blocking_recv() {
                let result = platform::delete(&address).map_err(|error| format!("{error:#}"));
                if sender.blocking_send(Event::Deleted(address, result)).is_err() { return; }
            }
        });
        if let Err(error) = started {
            let _ = fallback.try_send(Event::Listed(Err(format!("Penyimpanan profil tidak dapat dimulai: {error}"))));
        }
        (Self { jobs, busy: true }, events)
    }
    /// Call once an event drained from `events` (by the `app.rs` subscription
    /// bridge) has been handled, to clear the loading indicator.
    pub fn event_received(&mut self) { self.busy = false; }
    pub fn delete(&mut self, address: String) -> Result<()> {
        if self.busy { bail!("Tunggu penyimpanan profil selesai."); }
        self.jobs.try_send(Job::Delete(address)).context("Pekerjaan penyimpanan tidak tersedia")?;
        self.busy = true;
        Ok(())
    }
}

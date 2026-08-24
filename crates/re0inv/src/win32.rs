//! Win32 bindings.
//!
//! Declared by hand rather than pulled from a crate. The mod needs nine
//! functions, and the binding crates reach these through raw-dylib imports,
//! which the gnu target cannot build here (its bundled dlltool has no
//! assembler to call). Classic import-library linking has no such problem, and
//! this keeps the crate dependency-free.
//!
//! The game is 32-bit and so is this DLL; the struct layout below assumes it.

use std::ffi::c_void;

const _: () = assert!(
    std::mem::size_of::<usize>() == 4,
    "this mod only targets 32-bit Windows"
);

pub type Handle = *mut c_void;
pub type Bool = i32;

pub const TRUE: Bool = 1;

pub const DLL_PROCESS_ATTACH: u32 = 1;

/// Signature Win32 expects of a thread entry point.
pub type ThreadStart = unsafe extern "system" fn(*mut c_void) -> u32;

/// Layout for 32-bit Windows: seven 4-byte fields, no padding.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MemoryBasicInformation {
    pub base_address: *mut c_void,
    pub allocation_base: *mut c_void,
    pub allocation_protect: u32,
    pub region_size: usize,
    pub state: u32,
    pub protect: u32,
    pub kind: u32,
}

// --- Memory state and protection flags ---

pub const MEM_COMMIT: u32 = 0x1000;

pub const PAGE_NOACCESS: u32 = 0x01;
pub const PAGE_READONLY: u32 = 0x02;
pub const PAGE_READWRITE: u32 = 0x04;
pub const PAGE_WRITECOPY: u32 = 0x08;
pub const PAGE_EXECUTE_READ: u32 = 0x20;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;
pub const PAGE_EXECUTE_WRITECOPY: u32 = 0x80;
pub const PAGE_GUARD: u32 = 0x100;

/// Every protection value that permits reading.
pub const PAGE_READABLE: u32 = PAGE_READONLY
    | PAGE_READWRITE
    | PAGE_WRITECOPY
    | PAGE_EXECUTE_READ
    | PAGE_EXECUTE_READWRITE
    | PAGE_EXECUTE_WRITECOPY;

/// Every protection value that permits writing.
pub const PAGE_WRITABLE: u32 =
    PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;

/// Set in the high bit of GetAsyncKeyState's result while a key is held.
pub const KEY_PRESSED: u16 = 0x8000;

#[link(name = "kernel32")]
extern "system" {
    /// Changes page protection. `old` receives the previous value, which must
    /// be restored once the write is done.
    pub fn VirtualProtect(
        address: *const c_void,
        size: usize,
        new_protect: u32,
        old_protect: *mut u32,
    ) -> Bool;

    /// Discards anything the CPU cached from a code page that just changed.
    pub fn FlushInstructionCache(process: Handle, address: *const c_void, size: usize) -> Bool;

    pub fn GetModuleHandleA(module_name: *const u8) -> Handle;

    pub fn GetModuleFileNameA(module: Handle, filename: *mut u8, size: u32) -> u32;

    pub fn DisableThreadLibraryCalls(module: Handle) -> Bool;

    pub fn CreateThread(
        attributes: *const c_void,
        stack_size: usize,
        start: Option<ThreadStart>,
        parameter: *mut c_void,
        flags: u32,
        thread_id: *mut u32,
    ) -> Handle;

    pub fn CloseHandle(object: Handle) -> Bool;

    /// Pseudo-handle to the calling process. Needs no CloseHandle.
    pub fn GetCurrentProcess() -> Handle;

    pub fn ReadProcessMemory(
        process: Handle,
        address: *const c_void,
        buffer: *mut c_void,
        size: usize,
        bytes_read: *mut usize,
    ) -> Bool;

    pub fn VirtualQuery(
        address: *const c_void,
        buffer: *mut MemoryBasicInformation,
        length: usize,
    ) -> usize;
}

#[link(name = "user32")]
extern "system" {
    pub fn GetAsyncKeyState(key: i32) -> i16;
}

/// Silences an unused-constant warning while PAGE_NOACCESS is only documentation.
#[allow(dead_code)]
const _NOACCESS: u32 = PAGE_NOACCESS;

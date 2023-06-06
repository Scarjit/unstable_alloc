#![crate_type = "dylib"]
#![allow(unused_variables)]

use std::collections::{HashMap, VecDeque};
use winapi::um::memoryapi::{VirtualAlloc, VirtualFree, VirtualLock, VirtualUnlock  };
use winapi::um::winnt::{MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};
use winapi::shared::minwindef::*;
use winapi::ctypes::c_void;
use std::ptr;
use std::sync::Mutex;
use std::sync::{Once};

static mut HEAP: Mutex<*mut c_void> = Mutex::new(ptr::null_mut());
static mut FREE_BLOCKS: Mutex<VecDeque<usize>> = Mutex::new(VecDeque::new());

static mut ALLOCATED_BLOCK_PTR_MAP: *mut HashMap<*mut u8, usize> = ptr::null_mut();
static ALLOCATED_BLOCK_PTR_MAP_ONCE: Once = Once::new();

const HEAP_SIZE: usize = 8 * 1024 * 1024 * 1024; // 8GB
const BLOCK_SIZE: usize = 8; // 512B blocks
const NUM_BLOCKS: usize = HEAP_SIZE / BLOCK_SIZE;

#[no_mangle]
#[allow(non_snake_case, unused_variables)]
pub extern "system" fn DllMain(dll_module: HINSTANCE, call_reason: DWORD, reserved: LPVOID) -> BOOL {
    const DLL_PROCESS_ATTACH: DWORD = 1;
    const DLL_PROCESS_DETACH: DWORD = 0;

    match call_reason {
        DLL_PROCESS_ATTACH => {
            println!("DLL_PROCESS_ATTACH");
            unsafe {
                // Allocate 8GB of memory
                let heap_ptr = VirtualAlloc(ptr::null_mut(), HEAP_SIZE, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
                // Lock the page to prevent it from being swapped out
                let _ = VirtualLock(heap_ptr, HEAP_SIZE);

                *HEAP.lock().unwrap() = heap_ptr;

                // This is a hack to force the page to be allocated
                let page_size = 4096; // 4KB
                let heap_ptr = *HEAP.lock().unwrap();
                let heap_ptr = heap_ptr as *mut u8;
                for i in (0..(HEAP_SIZE)).step_by(page_size) {
                    *heap_ptr.offset(i as isize) = 1;
                }

                let mut free_blocks =  FREE_BLOCKS.lock().unwrap();
                free_blocks.extend(0..NUM_BLOCKS);
            }
        },
        DLL_PROCESS_DETACH => {
            println!("DLL_PROCESS_DETACH");
            unsafe {
                // Unlock the page
                let _ = VirtualUnlock(*HEAP.lock().unwrap(), HEAP_SIZE);
                // Free the page
                let _ = VirtualFree(*HEAP.lock().unwrap(), 0, MEM_RELEASE);
            }
        },
        _ => {
        }
    }
    return TRUE;
}

fn get_allocated_block_ptr_map() -> &'static mut HashMap<*mut u8, usize> {
    unsafe {
        ALLOCATED_BLOCK_PTR_MAP_ONCE.call_once(|| {
            ALLOCATED_BLOCK_PTR_MAP = Box::into_raw(Box::new(HashMap::new()));
        });
        &mut *ALLOCATED_BLOCK_PTR_MAP
    }
}

fn alloc(size: u64) -> *mut u8 {
    let num_blocks_needed = (size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let mut free_blocks = unsafe { FREE_BLOCKS.lock().unwrap() };

    if free_blocks.len() < num_blocks_needed {
        println!("Not enough memory available. Requested: {} blocks, Available: {} blocks", num_blocks_needed, free_blocks.len());
        // Not enough memory available
        return ptr::null_mut();
    }

    let block_start = free_blocks.drain(..num_blocks_needed).next().unwrap();
    let heap_start = unsafe { *HEAP.lock().unwrap() };

    // Add the ptr to the allocated block ptr map
    let allocated_block_ptr_map = get_allocated_block_ptr_map();
    let ptr = unsafe {
        heap_start.offset((block_start * BLOCK_SIZE) as isize) as *mut u8
    };
    allocated_block_ptr_map.insert(ptr, num_blocks_needed);

    return ptr;
}

fn free(ptr: *mut u8) {
    // Remove the ptr from the allocated block ptr map
    let allocated_block_ptr_map = get_allocated_block_ptr_map();
    let size = *allocated_block_ptr_map.get(&ptr).unwrap();

    let num_blocks_freed = (size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let heap_start = unsafe { *HEAP.lock().unwrap() };
    let block_start = ((ptr as isize) - (heap_start as isize)) / (BLOCK_SIZE as isize);

    let mut free_blocks = unsafe { FREE_BLOCKS.lock().unwrap() };
    free_blocks.extend(block_start as usize..(block_start as usize + num_blocks_freed));

    allocated_block_ptr_map.remove(&ptr);
}


#[no_mangle]
pub extern "C" fn MemTotalCommitted() -> u64 {
    HEAP_SIZE as u64
}

#[no_mangle]
pub extern "C" fn MemTotalReserved() -> u64 {
    HEAP_SIZE as u64
}

#[no_mangle]
pub extern "C" fn MemFlushCache(size: u64) -> u64{
    // This is a no-op since we alloc at start and never free
    size
}

#[no_mangle]
pub extern "C" fn MemFlushCacheAll() {
    // This is a no-op since we alloc at start and never free
}

#[no_mangle]
pub extern "C" fn MemSize(ptr: *mut u8) -> u64 {
    // Check the allocated block ptr map
    let allocated_block_ptr_map = get_allocated_block_ptr_map();
    let num_blocks = allocated_block_ptr_map.get(&ptr).unwrap();
    return (*num_blocks * BLOCK_SIZE) as u64;
}

#[no_mangle]
pub extern "C" fn MemAlloc(size: u64) -> *mut u8 {
    alloc(size)
}

#[no_mangle]
pub extern "C" fn MemFree(ptr: *mut u8) {
    free(ptr)
}

#[no_mangle]
pub extern "C" fn MemSizeA(ptr: *mut u8, alignment: u64) -> u64 {
    let size = MemSize(ptr);
    // Fit to alignment
    let aligned_size = (size + alignment - 1) & !(alignment - 1);
    aligned_size
}

#[no_mangle]
pub extern "C" fn MemAllocA(size: u64, alignment: u64) -> *mut u8 {
    // Calculate the aligned size by rounding up to the nearest multiple of alignment
    let aligned_size = (size + alignment - 1) & !(alignment - 1);
    alloc(aligned_size)
}

#[no_mangle]
pub extern "C" fn MemFreeA(ptr: *mut u8) {
    free(ptr)
}

#[no_mangle]
pub extern "C" fn EnableHugePages(enable: bool) {
    // Ignore this, we will always use huge pages
}

// Tests for the allocator
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc() {
        DllMain(ptr::null_mut(), 1, ptr::null_mut());
        let ptr = alloc(100);
        assert!(!ptr.is_null());
        assert_eq!(MemSize(ptr), BLOCK_SIZE as u64);
        assert_eq!(MemTotalCommitted(), HEAP_SIZE as u64);
        assert_eq!(MemTotalReserved(), HEAP_SIZE as u64);
        free(ptr);
        DllMain(ptr::null_mut(), 0, ptr::null_mut());
    }

    #[test]
    fn test_alloc_1gib(){
        DllMain(ptr::null_mut(), 1, ptr::null_mut());
        let ptr = alloc(1024 * 1024 * 1024);
        assert!(!ptr.is_null());
        assert_eq!(MemSize(ptr),1024 * 1024 * 1024);
        assert_eq!(MemTotalCommitted(), HEAP_SIZE as u64);
        assert_eq!(MemTotalReserved(), HEAP_SIZE as u64);
        free(ptr);
        DllMain(ptr::null_mut(), 0, ptr::null_mut());
    }

    #[test]
    fn test_alloc_1gib_uneven(){
        DllMain(ptr::null_mut(), 1, ptr::null_mut());
        let ptr = alloc(1024 * 1024 * 1024+3);
        assert!(!ptr.is_null());
        assert_eq!(MemSize(ptr),1024 * 1024 * 1024+BLOCK_SIZE as u64);
        assert_eq!(MemTotalCommitted(), HEAP_SIZE as u64);
        assert_eq!(MemTotalReserved(), HEAP_SIZE as u64);
        free(ptr);
        DllMain(ptr::null_mut(), 0, ptr::null_mut());
    }
}

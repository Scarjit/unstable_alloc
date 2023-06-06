use criterion::{criterion_group, criterion_main, Criterion};
use std::ffi::{c_ulong, c_void};
use libloading::Error;
use libloading::os::windows::*;

fn alloc(mem_alloc: Symbol<unsafe extern "C" fn(c_ulong) -> *mut c_void>, mem_free: Symbol<unsafe extern "C" fn(*mut c_void)>,mem_size: Symbol<unsafe extern "C" fn(*mut c_void) -> c_ulong>, size: u32) {
    let ptr = unsafe { mem_alloc(size) };
    assert_ne!(ptr, std::ptr::null_mut());
    // assert that the ptr has size bytes free using mem_size
    assert!(size <= unsafe { mem_size(ptr) });
    unsafe { mem_free(ptr) };
}

fn alloc_aligned(mem_alloc_a: Symbol<unsafe extern "C" fn(c_ulong, c_ulong) -> *mut c_void>, mem_free_a: Symbol<unsafe extern "C" fn(*mut c_void)>,mem_size_a: Symbol<unsafe extern "C" fn(*mut c_void, c_ulong) -> c_ulong>, size: u32, alignment: u32) {
    let ptr = unsafe { mem_alloc_a(size, alignment) };
    assert_ne!(ptr, std::ptr::null_mut());
    // assert that the ptr has size bytes free using mem_size_a
    assert!(size <= unsafe { mem_size_a(ptr, alignment.clone()) });
    unsafe { mem_free_a(ptr) };
}

fn alloc_free_and_flush(mem_alloc: Symbol<unsafe extern "C" fn(c_ulong) -> *mut c_void>, mem_free: Symbol<unsafe extern "C" fn(*mut c_void)>, mem_size: Symbol<unsafe extern "C" fn(*mut c_void) -> c_ulong>, mem_flush_cache: Symbol<unsafe extern "C" fn(c_ulong) -> c_ulong>, size: u32) {
    let ptr = unsafe { mem_alloc(size) };
    assert_ne!(ptr, std::ptr::null_mut());
    // assert that the ptr has size bytes free using mem_size
    assert!(size <= unsafe { mem_size(ptr) });
    unsafe { mem_free(ptr) };
    unsafe { mem_flush_cache(ptr as c_ulong) };
}


const ALIGNMENTS: [u32; 3] = [8, 32, 64];
const SIZES: [u32;2] = [8,1024*1024*1024];
//const ALLOCATORS: [&str; 3] = ["./allocators/mimalloc_v212_lock_pages.dll", "./allocators/jemalloc_bi_x64.dll", "./allocators/tbb4malloc_bi_x64.dll"];
const ALLOCATORS: [&str; 1] = ["./target/debug/allocbench.dll"];


fn bench(c: &mut Criterion){
    for allocator in ALLOCATORS.iter(){
        println!("Running benchmarks for {}", allocator);
        unsafe {
            let lib: Library = Library::new(allocator).expect("Failed to load library");

            let mem_total_committed: Symbol<unsafe extern "C" fn() -> c_ulong> = lib.get(b"MemTotalCommitted").expect("Failed to get function (MemTotalCommitted)");
            let mem_total_reserved: Symbol<unsafe extern "C" fn() -> c_ulong> = lib.get(b"MemTotalReserved").expect("Failed to get function (MemTotalReserved)");
            let mem_flush_cache: Symbol<unsafe extern "C" fn(c_ulong) -> c_ulong> = lib.get(b"MemFlushCache").expect("Failed to get function (MemFlushCache)");
            let mem_flush_cache_all: Symbol<unsafe extern "C" fn()> = lib.get(b"MemFlushCacheAll").expect("Failed to get function (MemFlushCacheAll)");
            let mem_size: Symbol<unsafe extern "C" fn(*mut c_void) -> c_ulong> = lib.get(b"MemSize").expect("Failed to get function (MemSize)");
            let mem_alloc: Symbol<unsafe extern "C" fn(c_ulong) -> *mut c_void> = lib.get(b"MemAlloc").expect("Failed to get function (MemAlloc)");
            let mem_free: Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"MemFree").expect("Failed to get function (MemFree)");
            let mem_size_a: Symbol<unsafe extern "C" fn(*mut c_void, c_ulong) -> c_ulong> = lib.get(b"MemSizeA").expect("Failed to get function (MemSizeA)");
            let mem_alloc_a: Symbol<unsafe extern "C" fn(c_ulong, c_ulong) -> *mut c_void> = lib.get(b"MemAllocA").expect("Failed to get function (MemAllocA)");
            let mem_free_a: Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"MemFreeA").expect("Failed to get function (MemFreeA)");


            for size in SIZES.iter() {
                c.bench_function(format!("{}-alloc_{}B", allocator, size).as_str(), |b| b.iter(|| alloc(mem_alloc.clone(), mem_free.clone(), mem_size.clone(),size.clone())));
                c.bench_function(format!("{}-alloc_free_flush_{}B", allocator,size).as_str(), |b| b.iter(|| alloc_free_and_flush(mem_alloc.clone(), mem_free.clone(), mem_size.clone(), mem_flush_cache.clone(), size.clone())));
                for alignment in ALIGNMENTS.iter() {
                    c.bench_function(format!("{}-alloc_aligned_{}B_{}A", allocator, size, alignment).as_str(), |b| b.iter(|| alloc_aligned(mem_alloc_a.clone(), mem_free_a.clone(), mem_size_a.clone(),size.clone(), alignment.clone())));
                }
            }
            c.bench_function(&format!("{}-mem_total_committed", allocator), |b| b.iter(|| mem_total_committed()));

            // Enable huge pages

             match lib.get::<Symbol<unsafe extern "C" fn()>>(b"EnableHugePages") {
                Ok(v) => {
                    v();

                    for size in SIZES.iter() {
                        c.bench_function(format!("{}-hp-alloc_{}B", allocator, size).as_str(), |b| b.iter(|| alloc(mem_alloc.clone(), mem_free.clone(), mem_size.clone(),size.clone())));
                        c.bench_function(format!("{}-alloc_free_flush_{}B", allocator,size).as_str(), |b| b.iter(|| alloc_free_and_flush(mem_alloc.clone(), mem_free.clone(), mem_size.clone(), mem_flush_cache.clone(), size.clone())));
                        for alignment in ALIGNMENTS.iter() {
                            c.bench_function(format!("{}-hp-alloc_aligned_{}B_{}A", allocator, size, alignment).as_str(), |b| b.iter(|| alloc_aligned(mem_alloc_a.clone(), mem_free_a.clone(), mem_size_a.clone(),size.clone(), alignment.clone())));
                        }
                    }
                    c.bench_function(&format!("{}-hp-mem_total_committed", allocator), |b| b.iter(|| mem_total_committed()));

                }
                 Err(_) => {
                     println!("Failed to get function (EnableHugePages)");
                 }
            }

            lib.close().expect("Failed to close library");
        }
    }


}

criterion_group!(benches, bench);
criterion_main!(benches);

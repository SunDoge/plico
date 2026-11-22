//! Versioned, bounded request/response buffers for trusted Rust development modules.
pub use rkyv;
use rkyv::{
    api::high::HighSerializer,
    rancor::Error as ArchiveError,
    ser::{allocator::ArenaHandle, writer::Buffer},
};

/// Write the archive directly into a caller-owned transfer buffer.
pub fn write_archive<'b, T>(
    value: &T,
    output: &'b mut [u8],
) -> std::result::Result<usize, ArchiveError>
where
    T: for<'a> rkyv::Serialize<HighSerializer<Buffer<'b>, ArenaHandle<'a>, ArchiveError>>,
{
    Ok(rkyv::api::high::to_bytes_in::<_, ArchiveError>(value, Buffer::from(output))?.len())
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
mod host;
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
pub use host::*;
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
mod reload;
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
pub use reload::Reloads;

/// Export a synchronous handler from one wasm32 module. The application assigns
/// a protocol version shared with its host and writes its reply into the output slice.
#[macro_export]
macro_rules! export_ui {
    ($abi:expr, $capacity:expr, $input:ty, $handle:expr) => {
        $crate::export_buffers!($abi, $capacity, |bytes: &[u8], output: &mut [u8]| {
            let Ok(input) = $crate::rkyv::access::<
                $crate::rkyv::Archived<$input>,
                $crate::rkyv::rancor::Error,
            >(bytes) else {
                return 0;
            };
            ($handle)(input, output)
        });
    };
}

/// Export a handler that consumes and produces the protocol's byte layout in place.
#[macro_export]
macro_rules! export_buffers {
    ($abi:expr, $capacity:expr, $handle:expr) => {
        ::std::thread_local! {
            static INPUT: ::std::cell::RefCell<[u8; $capacity]> = const { ::std::cell::RefCell::new([0; $capacity]) };
            static OUTPUT: ::std::cell::RefCell<[u8; $capacity]> = const { ::std::cell::RefCell::new([0; $capacity]) };
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn abi_version() -> u32 { $abi }

        #[unsafe(no_mangle)]
        pub extern "C" fn buffer_capacity() -> u32 { $capacity as u32 }

        #[unsafe(no_mangle)]
        pub extern "C" fn input_ptr() -> u32 {
            INPUT.with(|input| input.borrow().as_ptr() as u32)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn output_ptr() -> u32 {
            OUTPUT.with(|output| output.borrow().as_ptr() as u32)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn update_ui(len: u32) -> u32 {
            INPUT.with_borrow(|input| OUTPUT.with_borrow_mut(|output| {
                let Some(bytes) = input.get(..len as usize) else { return 0 };
                ($handle)(bytes, output.as_mut_slice())
            }))
        }
    };
}

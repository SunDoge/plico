//! Private immutable drawing effects; DirectWrite owns their COM references.
#![allow(non_snake_case, non_camel_case_types)]
use windows::core::*;

#[interface("1d9772bc-5129-48a4-a1c8-09fd1ca14c95")]
unsafe trait ITextColor: IUnknown {
    fn GetColor(&self, value: *mut u32) -> HRESULT;
}
#[implement(ITextColor, Agile = false)]
struct TextColor(u32);
#[allow(non_snake_case)]
impl ITextColor_Impl for TextColor_Impl {
    unsafe fn GetColor(&self, value: *mut u32) -> HRESULT {
        if value.is_null() {
            return windows::Win32::Foundation::E_POINTER;
        }
        // SAFETY: caller supplies writable storage for one packed RGBA value.
        unsafe {
            *value = self.0;
        }
        HRESULT(0)
    }
}
pub(super) fn effect(color: [u8; 4]) -> Result<IUnknown> {
    let object: ITextColor = TextColor(u32::from_le_bytes(color)).into();
    object.cast()
}
pub(super) fn read(effect: Ref<IUnknown>) -> Result<Option<[u8; 4]>> {
    let Some(effect) = effect.as_ref() else {
        return Ok(None);
    };
    let effect: ITextColor = effect.cast()?;
    let mut packed = 0;
    // SAFETY: typed private interface, writable local output, synchronous call.
    unsafe { effect.GetColor(&mut packed) }.ok()?;
    Ok(Some(packed.to_le_bytes()))
}

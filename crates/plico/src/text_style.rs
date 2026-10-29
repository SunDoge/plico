//! Shared, compile-time text setters for builders and reactive property editors.

/// Generate the common text-style methods with an owned or borrowed receiver.
/// Field names match `Style`; validation remains at the Engine mutation boundary.
#[doc(hidden)]
#[macro_export]
macro_rules! __plico_text_style_methods {
    ($mode:ident $path:tt) => {
        $crate::__plico_text_style_methods!(@catalog $mode $path;
            /// Set the foreground color token.
            color($crate::Color) => foreground;
            /// Set the font size in logical pixels. Zero is invalid.
            font_size(u16) => font_size;
            /// Set the font weight, in the inclusive range 100–900.
            weight(u16) => font_weight;
            /// Select a portable generic font family.
            font_family($crate::FontFamily) => font_family;
            /// Set opacity in the inclusive range 0–1. Non-finite values are invalid.
            opacity(f32) => opacity;
        );
    };
    (@catalog $mode:ident $path:tt; $(
        $(#[$doc:meta])* $name:ident($ty:ty) => $field:ident;
    )*) => {
        $(
            $crate::__plico_text_style_methods!(
                @method $mode $path [$(#[$doc])*] $name($ty) => $field
            );
        )*
    };
    (@method owned ($($path:ident).+) [$(#[$doc:meta])*] $name:ident($ty:ty) => $field:ident) => {
        $(#[$doc])*
        #[inline(always)]
        pub fn $name(mut self, value: $ty) -> Self {
            self.$($path).+.$field = value;
            self
        }
    };
    (@method borrowed ($($path:ident).+) [$(#[$doc:meta])*] $name:ident($ty:ty) => $field:ident) => {
        $(#[$doc])*
        #[inline(always)]
        pub fn $name(&mut self, value: $ty) -> &mut Self {
            self.$($path).+.$field = value;
            self
        }
    };
}

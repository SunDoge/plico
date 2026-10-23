//! Minimal link probe: the executable references only SAVE, not every Lucide icon.
#[allow(dead_code)]
#[path = "../tests/support/icons.rs"]
mod icons;
fn main() {
    println!("{}", icons::SAVE.source());
}

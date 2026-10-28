//! Ask the desktop portal for a screenshot and print its image URI.
#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), plico_native::capture::Error> {
    let screenshot = plico_native::capture::screenshot().await?;
    println!("{}", screenshot.uri);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The screenshot portal is currently supported on Linux only.");
}

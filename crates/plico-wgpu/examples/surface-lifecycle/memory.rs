//! Process-level observations, not renderer allocation accounting or a leak gate.
#[cfg(target_os = "macos")]
pub(super) fn report(phase: &str) -> Result<(), super::CheckError> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v0>::zeroed();
    // SAFETY: RUSAGE_INFO_V0 writes a full rusage_info_v0 to this aligned,
    // writable buffer. libproc's historical API types the buffer as void **,
    // but takes the address of the structure itself, not an indirect pointer.
    let status = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V0,
            usage.as_mut_ptr().cast(),
        )
    };
    if status != 0 {
        return Err(super::error(format!(
            "process memory sample {phase}: {}",
            std::io::Error::last_os_error()
        )));
    }
    // SAFETY: Successful proc_pid_rusage initialized the requested V0 fields.
    let usage = unsafe { usage.assume_init() };
    println!(
        "surface-memory: phase={phase} rss_bytes={} footprint_bytes={}",
        usage.ri_resident_size, usage.ri_phys_footprint
    );
    Ok(())
}
#[cfg(not(target_os = "macos"))]
pub(super) fn report(_phase: &str) -> Result<(), super::CheckError> {
    Ok(())
}

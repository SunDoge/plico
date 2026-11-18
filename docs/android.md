# Android

The Android counter uses the same Plico widgets, runtime and wgpu rendering as
desktop applications. Its NativeActivity entry point passes a frame source to
`plico_android::run`. The host keeps the source alive across surface recreation.

## Run the example

Install the Android SDK platform and build tools for API 35, platform-tools,
an NDK and a JDK. Set `ANDROID_HOME` to the SDK and `ANDROID_NDK_ROOT` to the
installed NDK directory. Start an emulator or connect a device with USB debugging.
From the workspace root run:

```sh
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-apk --version 0.10.0 --locked
scripts/build-android-counter.sh
adb install -r target/debug/apk/android-counter.apk
adb shell am start -W -n dev.plico.counter/dev.plico.PlicoActivity
```

The debug APK contains ARM64 and x86_64 libraries. Use the counter to check
activation, rotate the device to check layout, and switch away and return to
check surface recreation. The note editor echoes its buffer below the field.
The slider displays the current value below its track. Tap **Increment in 2
seconds** and leave the screen idle to exercise the host's deadline-based
redraw. The count updates when the deadline arrives.

The example has been exercised on an Android 16 x86_64 emulator. ARM64 has
build and packaging coverage. The build script packages the PlicoActivity Java input bridge alongside the native libraries. A focused editor exposes an Android InputConnection and forwards composition, commits and surrounding-text deletion to the Rust event loop. On the Android 16 x86_64 emulator, Fcitx5 Pinyin
was tested by tapping the keys for `nihao`, selecting **你好**, checking the
note echo, and deleting the final character.

The focused note supports plain-text cut, copy and paste through Android's
clipboard. **Open Plico on GitHub** hands an HTTPS link to the system browser.

The shader strip provides a visual check for two render passes. The top half
draws red, green and blue. The lower half samples that background and rotates
its color channels, yielding blue, red and green. The panel below it runs the
gallery's animated fluid-dots shader.

## Regression checks

Run `bash scripts/check-android.sh` with both Rust targets installed. It tests
the shared platform contracts, type-checks the app for both ABIs and checks
the resolved dependency graphs. The Android host can use winit while the
`plico-wgpu/surface` renderer can build independently of it.

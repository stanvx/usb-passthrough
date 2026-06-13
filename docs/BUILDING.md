# USB/IP Passthrough — Building from Source

Complete guide to compiling the USB/IP passthrough project for all target platforms.

> **Workspace root:** `/home/localadmin/usb-passthrough`  
> **Rust workspace:** `Cargo.toml` with 4 crates (core, server, client, windows)  
> **Android:** Gradle + rust-android-gradle plugin

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Project Structure](#project-structure)
- [Building the Rust Core (All Platforms)](#building-the-rust-core-all-platforms)
- [Building for Linux](#building-for-linux)
- [Building for Windows](#building-for-windows)
- [Building for Android](#building-for-android)
- [Signing APKs](#signing-apks)
- [Building Windows Installer (NSIS)](#building-windows-installer-nsis)
- [Cross-Compilation](#cross-compilation)
- [Continuous Integration](#continuous-integration)
- [Troubleshooting Builds](#troubleshooting-builds)

---

## Prerequisites

### Rust Toolchain (All Platforms)

```bash
# Install rustup if not already present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify
rustc --version    # Should be 1.75+
cargo --version

# Add cross-compilation targets (optional, for cross-building)
rustup target add x86_64-pc-windows-gnu       # Linux → Windows
rustup target add aarch64-linux-android        # Linux → Android ARM64
rustup target add armv7-linux-androideabi      # Linux → Android ARM32
rustup target add x86_64-linux-android         # Linux → Android x86_64
```

### Linux Build Dependencies

```bash
sudo apt update
sudo apt install \
    build-essential \
    pkg-config \
    libusb-1.0-0-dev \
    libudev-dev \
    clang \
    libclang-dev \
    llvm-dev \
    cmake
```

### Windows Build Dependencies

```powershell
# Install Rust for Windows
# Download from: https://rustup.rs

# Install Visual Studio Build Tools 2022
# Download from: https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022
# Required workloads:
#   - .NET desktop build tools
#   - Desktop development with C++
#   - Windows 10/11 SDK
```

**For cross-compiling from Linux to Windows:**

```bash
sudo apt install mingw-w64
rustup target add x86_64-pc-windows-gnu
```

### Android Build Dependencies

```bash
# Android SDK — required for building the APK
# Option 1: Android Studio (download from developer.android.com)
# Option 2: Command-line tools

# Download command-line tools
wget https://dl.google.com/android/repository/commandlinetools-linux-9477386_latest.zip
unzip commandlinetools-linux-*.zip
mkdir -p ~/Android/cmdline-tools
mv cmdline-tools ~/Android/cmdline-tools/latest

# Set environment variables
export ANDROID_HOME=$HOME/Android
export PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin

# Accept licenses and install SDK
yes | sdkmanager --licenses
sdkmanager "platforms;android-34" "build-tools;34.0.0" "ndk;25.2.9519653"

# Set NDK path (used by rust-android-gradle)
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/25.2.9519653
```

### JDK (for Android Builds)

```bash
# Install JDK 17
sudo apt install openjdk-17-jdk

# Verify
java --version
```

---

## Project Structure

```
usb-passthrough/
├── Cargo.toml                    # Workspace root
├── shared/
│   └── usbip-core/              # Core protocol types, crypto, errors
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs           # USBIP_PORT, MAX_MESSAGE_SIZE, speed enum
│           ├── protocol.rs     # Wire-format USB/IP headers
│           ├── urb.rs          # URB types, buffer pooling
│           ├── crypto.rs       # AES-256-GCM, X25519 ECDH
│           └── error.rs        # Error types
├── server/
│   └── usbip-server/           # USB/IP server binary
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs         # CLI with clap, server entry point
│           ├── lib.rs
│           ├── server.rs       # TCP listener, client handling
│           ├── usb.rs          # USB device access via libusb
│           └── discovery.rs    # mDNS advertisement
├── client/
│   └── usbip-client/           # USB/IP client binary
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs         # CLI with clap
│           ├── lib.rs
│           ├── client.rs       # TCP connection, import protocol
│           ├── vhci.rs         # VHCI driver / uinput
│           └── discovery.rs    # mDNS browsing
├── windows/
│   ├── Cargo.toml              # Windows egui app + service
│   └── src/
│       ├── main.rs             # egui tray app + Windows Service
│       └── windows_usb.rs      # Win32 SetupAPI enumeration
├── android/
│   ├── build.gradle.kts        # Root Gradle (AGP 8.2, Kotlin 1.9.20, rust-android 0.9.4)
│   ├── settings.gradle.kts     # App + TV modules
│   ├── app/
│   │   ├── build.gradle.kts    # Phone/tablet app module
│   │   └── src/main/java/...   # Jetpack Compose UI, foreground service
│   └── tv/
│       └── src/main/java/...   # Android TV Compose UI
├── ROADMAP.md
├── ARCHITECTURE.md
├── PROTOCOL.md
└── README.md
```

### Crate Dependency Tree

```
usbip-core           (shared protocol types — no platform deps)
├── ring             (AES-256-GCM, X25519)
├── zerocopy         (safe transmutation for wire types)
├── byteorder        (big-endian types)
├── crc32fast        (CRC-32 for descriptor verification)
└── serde/serde_json (config serialization)

usbip-server         (server binary)
├── usbip-core       (protocol types)
├── rusb             (libusb bindings)
├── tokio            (async TCP)
├── mdns-sd          (mDNS advertisement)
└── clap             (CLI parsing)

usbip-client         (client binary)
├── usbip-core       (protocol types)
├── tokio            (async TCP)
├── mdns-sd          (mDNS browsing)
└── clap             (CLI parsing)

windows              (Windows GUI + Service)
├── usbip-core       (protocol types)
├── eframe/egui      (GUI framework)
├── winapi           (Win32 USB APIs, service API)
├── windows-service  (NT service framework)
└── clap             (CLI parsing)

android (Kotlin)     (Android app)
├── Rust (via JNI)   (usbip-core + usbip-client + usbip-server)
├── Jetpack Compose  (UI)
├── AndroidX TV      (TV UI components)
└── OkHttp           (future: HTTP fallback)
```

---

## Building the Rust Core (All Platforms)

### Build All Crates

```bash
cd /home/localadmin/usb-passthrough

# Build workspace (all crates)
cargo build --release

# Test workspace (runs unit tests for all crates)
cargo test --release

# Run specific crate tests
cargo test --release -p usbip-core
cargo test --release -p usbip-server
cargo test --release -p usbip-client
cargo test --release -p windows
```

### Core Crate Tests

The `usbip-core` crate has comprehensive tests:

```bash
cargo test --release -p usbip-core

# Running tests (output):
#   test crypto::tests::test_keypair_generation ... ok
#   test crypto::tests::test_ecdh_agreement ... ok
#   test crypto::tests::test_encrypt_decrypt ... ok
#   test crypto::tests::test_hex_roundtrip ... ok
#   test crypto::tests::test_derive_session_key_hex ... ok
#   test crypto::tests::test_usbip_message_encrypt_decrypt ... ok
#   test crypto::tests::test_x25519_test_vector ... ok
```

### Build a Specific Crate

```bash
# Build just the server
cargo build --release -p usbip-server

# Build just the client
cargo build --release -p usbip-client

# Build just the Windows app
cargo build --release -p usbip-windows
```

### Output Locations

```
target/release/usbip-server      # Linux / macOS server
target/release/usbip-client      # Linux / macOS client
target/release/usbip-server.exe  # Windows server
target/release/usbip-client.exe  # Windows client
target/release/usbip-windows.exe # Windows egui app
```

---

## Building for Linux

### Native Build (x86_64)

```bash
cd /home/localadmin/usb-passthrough

# Build server
cargo build --release -p usbip-server

# Build client
cargo build --release -p usbip-client

# Install system-wide
sudo cp target/release/usbip-server /usr/local/bin/
sudo cp target/release/usbip-client /usr/local/bin/
```

### ARM64 (aarch64) Build

**On an ARM64 machine (Raspberry Pi, etc.):**

```bash
# Rust is already installed
cargo build --release -p usbip-server
```

**Cross-compile from x86_64:**

```bash
sudo apt install gcc-aarch64-linux-gnu
rustup target add aarch64-unknown-linux-gnu

cat >> ~/.cargo/config.toml << 'EOF'
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
EOF

cargo build --release --target aarch64-unknown-linux-gnu -p usbip-server
cargo build --release --target aarch64-unknown-linux-gnu -p usbip-client
```

### ARMv7 (armv7) Build (32-bit Raspberry Pi)

```bash
sudo apt install gcc-arm-linux-gnueabihf
rustup target add armv7-unknown-linux-gnueabihf

cat >> ~/.cargo/config.toml << 'EOF'
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
EOF

cargo build --release --target armv7-unknown-linux-gnueabihf -p usbip-server
```

---

## Building for Windows

### Option A: Native Build on Windows

**On a Windows machine with Rust installed:**

```powershell
cd C:\usb-passthrough

# Build all Windows components
cargo build --release -p windows
cargo build --release -p usbip-server
cargo build --release -p usbip-client
```

### Option B: Cross-Compile from Linux (MinGW)

```bash
sudo apt install mingw-w64
rustup target add x86_64-pc-windows-gnu

# Configure MinGW linker
cat >> ~/.cargo/config.toml << 'EOF'
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
EOF

cargo build --release --target x86_64-pc-windows-gnu -p windows
cargo build --release --target x86_64-pc-windows-gnu -p usbip-server
cargo build --release --target x86_64-pc-windows-gnu -p usbip-client
```

**Note:** Cross-compiled builds from Linux will be MinGW-linked and may require additional DLLs on the target machine. Native Windows builds with MSVC are preferred for production use.

### Output

```
target/x86_64-pc-windows-gnu/release/usbip-server.exe
target/x86_64-pc-windows-gnu/release/usbip-client.exe
target/x86_64-pc-windows-gnu/release/usbip-windows.exe
```

---

## Building for Android

### Prerequisites Checklist

```bash
# Verify all Android build dependencies
which java           # Should find JDK
echo $ANDROID_HOME   # Should point to SDK root
echo $ANDROID_NDK_HOME  # Should point to NDK root
rustup target list --installed | grep android  # Should show targets
```

### Quick Build

```bash
cd /home/localadmin/usb-passthrough/android

# Build all variants (debug + release)
./gradlew assemble

# Build just release
./gradlew assembleRelease

# Build phone/tablet app only
./gradlew :app:assembleRelease

# Build Android TV app only
./gradlew :tv:assembleRelease
```

### How the Android Build Works

The build uses the `org.mozilla.rust-android-gradle` plugin (v0.9.4, configured in `android/build.gradle.kts`):

```kotlin
plugins {
    id("com.android.application") version "8.2.0" apply false
    id("org.jetbrains.kotlin.android") version "1.9.20" apply false
    id("org.mozilla.rust-android-gradle.rust-android") version "0.9.4" apply false
}
```

The plugin automatically:
1. Cross-compiles the Rust libraries for each Android ABI
2. Links them into the APK as native `.so` files
3. Registers JNI entry points

### Rust JNI Bridge

The bridge is defined in `android/app/src/main/java/com/usbpassthrough/bridge/RustBridge.kt`:

```kotlin
object RustBridge {
    init {
        System.loadLibrary("usbip_jni")
    }
    
    external fun init()
    external fun startServer(deviceFd: Int, port: Int): Boolean
    external fun stopServer()
    external fun startClient(host: String, port: Int, busId: String): Boolean
    external fun stopClient()
    external fun submitUrb(seqnum: Int, devid: Int, direction: Int,
                           ep: Int, flags: Int, dataLen: Int,
                           setup: ByteArray, data: ByteArray): IntArray
}
```

The corresponding Rust side (in `shared/usbip-core/src/lib.rs` or a JNI shim crate) uses the `jni` crate.

### Build Output

```
android/app/build/outputs/apk/release/usb-passthrough-app-release.apk    # Phone APK
android/tv/build/outputs/apk/release/usb-passthrough-tv-release.apk      # TV APK
```

### Build Profile

For development, use debug builds (faster compilation, no optimizations):

```bash
./gradlew assembleDebug
```

Debug APK location:
```
android/app/build/outputs/apk/debug/usb-passthrough-app-debug.apk
```

### Building for Specific ABIs

By default, the build targets all Android ABIs. To limit scope:

```bash
# Build only for arm64-v8a (faster)
./gradlew assembleRelease -PtargetABIs=arm64-v8a

# Build for all except x86 (saves time if you don't need emulators)
./gradlew assembleRelease -PtargetABIs=arm64-v8a,armeabi-v7a
```

---

## Signing APKs

### Generate a Signing Key

```bash
keytool -genkey -v -keystore ~/android-keystore.jks \
  -alias usb-passthrough -keyalg RSA -keysize 2048 \
  -validity 10000

# You'll be prompted for:
# - Keystore password
# - Key password
# - Your name, organization, etc.
```

### Configure Signing in Gradle

Create or edit `android/app/signing.properties`:

```properties
storeFile=/home/yourname/android-keystore.jks
storePassword=your-store-password
keyAlias=usb-passthrough
keyPassword=your-key-password
```

Then the `build.gradle.kts` can read it:

```kotlin
android {
    signingConfigs {
        create("release") {
            val props = Properties()
            props.load(File("signing.properties").inputStream())
            storeFile = File(props["storeFile"].toString())
            storePassword = props["storePassword"].toString()
            keyAlias = props["keyAlias"].toString()
            keyPassword = props["keyPassword"].toString()
        }
    }
    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("release")
        }
    }
}
```

### Sign Manually (without Gradle config)

```bash
cd android

# Build unsigned APK
./gradlew :app:assembleRelease

# Sign with apksigner
$ANDROID_HOME/build-tools/34.0.0/apksigner sign \
  --ks ~/android-keystore.jks \
  --ks-key-alias usb-passthrough \
  --out usb-passthrough-signed.apk \
  app/build/outputs/apk/release/usb-passthrough-app-release-unsigned.apk

# Verify signature
$ANDROID_HOME/build-tools/34.0.0/apksigner verify usb-passthrough-signed.apk
```

---

## Building Windows Installer (NSIS)

### Prerequisites

```bash
# Install NSIS on Windows
# Download from: https://nsis.sourceforge.io/Download
# Or via Chocolatey:
choco install nsis

# Include the following NSIS plugins:
# - nsProcess (check if service is running)
# - SimpleFC (firewall rules)
```

### Installer Script

The installer script is at `windows/installer/usb-passthrough.nsi`:

```nsis
; usb-passthrough.nsi (partial)
Name "USB Passthrough"
OutFile "USB-Passthrough-Setup.exe"
InstallDir "$PROGRAMFILES64\USB Passthrough"

Section "Server (required)"
  SetOutPath "$INSTDIR"
  File "..\target\release\usbip-server.exe"
  File "..\target\release\usbip-client.exe"
  File "..\target\release\usbip-windows.exe"

  ; Install VHCI driver
  ExecWait '"$INSTDIR\drivers\dpinst.exe" /sw /sa /path "$INSTDIR\drivers"'

  ; Create firewall rule
  SimpleFC::AddPort 3240 "USB Passthrough" 6 1 "" 1
SectionEnd

Section "Start Menu Shortcuts"
  CreateShortCut "$SMPROGRAMS\USB Passthrough\USB Passthrough GUI.lnk" "$INSTDIR\usbip-windows.exe"
  CreateShortCut "$SMPROGRAMS\USB Passthrough\Uninstall.lnk" "$INSTDIR\uninstall.exe"
SectionEnd
```

### Build the Installer

```powershell
# 1. First build the Rust binaries (release mode)
cd C:\usb-passthrough
cargo build --release -p windows
cargo build --release -p usbip-server
cargo build --release -p usbip-client

# 2. Run NSIS
& "C:\Program Files (x86)\NSIS\makensis.exe" windows/installer/usb-passthrough.nsi

# 3. Output: windows/installer/USB-Passthrough-Setup.exe
```

### Cross-Platform Installer Build

Building the Windows installer requires running NSIS on Windows. For CI, this is typically done on a Windows runner.

---

## Cross-Compilation

### Full Cross-Compile Matrix

| Host → Target | Command |
|---------------|---------|
| x86_64 Linux → x86_64 Linux | `cargo build --release` |
| x86_64 Linux → aarch64 Linux | `cargo build --release --target aarch64-unknown-linux-gnu` |
| x86_64 Linux → armv7 Linux | `cargo build --release --target armv7-unknown-linux-gnueabihf` |
| x86_64 Linux → x86_64 Windows | `cargo build --release --target x86_64-pc-windows-gnu` |
| x86_64 Linux → aarch64 Android | Via Gradle (rust-android plugin) |
| x86_64 Linux → x86_64 Android | Via Gradle (rust-android plugin) |
| x86_64 Windows → x86_64 Windows | `cargo build --release` (MSVC) |
| macOS x86_64 → macOS x86_64 | `cargo build --release` |
| macOS ARM64 → macOS ARM64 | `cargo build --release` |

### Cross-Compilation .cargo/config.toml

Full example configuration for cross-compilation:

```toml
# ~/.cargo/config.toml

[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"

[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"

[target.x86_64-pc-windows-msvc]
linker = "link.exe"  # Requires Visual Studio

# Environment variables for Android cross-compilation
[env]
ANDROID_HOME = "/home/user/Android"
ANDROID_NDK_HOME = "/home/user/Android/ndk/25.2.9519653"
```

---

## Continuous Integration

### CI Pipeline Overview

The project uses a CI/CD pipeline (GitHub Actions) defined in `.github/workflows/ci.yml`:

```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo test --release

  build-linux:
    runs-on: ubuntu-latest
    steps:
      - run: cargo build --release -p usbip-server
      - run: cargo build --release -p usbip-client

  build-windows:
    runs-on: windows-latest
    steps:
      - run: cargo build --release -p windows
      - run: cargo build --release -p usbip-server
      - run: cargo build --release -p usbip-client

  build-android:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/setup-java@v4
        with:
          distribution: 'temurin'
          java-version: '17'
      - run: ./gradlew assembleRelease
```

### CI Build Artifacts

The pipeline produces:

```
usbip-server-x86_64-linux.tar.gz      # Linux server binary
usbip-client-x86_64-linux.tar.gz      # Linux client binary
usbip-server-x86_64-windows.zip       # Windows server binary
usbip-client-x86_64-windows.zip       # Windows client binary
usbip-windows-x86_64-windows.zip      # Windows GUI app + service
usb-passthrough-app-release.apk       # Android phone APK
usb-passthrough-tv-release.apk        # Android TV APK
USB-Passthrough-Setup.exe             # Windows installer
```

---

## Troubleshooting Builds

### Rust Build Errors

| Error | Cause | Fix |
|-------|-------|-----|
| `error: linker 'cc' not found` | Missing C compiler | `sudo apt install build-essential` or install Xcode CLT on macOS |
| `error: failed to run custom build command for 'ring v0.17.x'` | Missing `ring` build deps | Install `clang`, `libclang-dev` (Linux) or VS Build Tools (Windows) |
| `error[E0432]: import unresolved: rusb` | libusb not installed | `sudo apt install libusb-1.0-0-dev pkg-config` |
| `error: could not find native static library usb-1.0` | Same as above | Also try: `./configure && make && sudo make install` in libusb source |
| `error: the `i686-pc-windows-gnu` target may not be installed` | Missing Rust target | `rustup target add i686-pc-windows-gnu` (or the correct target) |

### Android Build Errors

| Error | Cause | Fix |
|-------|-------|-----|
| `No toolchains found in the NDK` | NDK not installed | `sdkmanager "ndk;25.2.9519653"` |
| `Execution failed for task ':app:mergeReleaseNativeLibs'` | Duplicate .so files | Clean build: `./gradlew clean` |
| `Could not find com.android.tools.build:gradle:8.2.0` | AGP not downloaded | Check internet; try `./gradlew --refresh-dependencies` |
| `Rust build failed for target aarch64-linux-android` | Missing Rust target | `rustup target add aarch64-linux-android` |
| `java.lang.UnsatisfiedLinkError` at runtime | JNI library not loaded | Check `System.loadLibrary("usbip_jni")` matches the lib name |

### Windows Build Errors

| Error | Cause | Fix |
|-------|-------|-----|
| `link: fatal error LNK1181: cannot open input file 'kernel32.lib'` | Windows SDK not installed | Install Visual Studio and select "Windows 10/11 SDK" |
| `error: could not find 'windows-service' crate` | Dependency issue | `cargo update` and try again |
| `thread 'main' panicked at 'egui requires linking with opengl'` | No OpenGL on this system | Ensure GPU drivers are installed; or use the `wgl` backend |

---

## Quick Build Reference

```bash
# === LINUX ===
sudo apt install libusb-1.0-0-dev pkg-config clang libclang-dev
cargo build --release -p usbip-server
cargo build --release -p usbip-client

# === WINDOWS (native) ===
# Install VS Build Tools, then:
cargo build --release -p windows

# === ANDROID ===
export ANDROID_HOME=$HOME/Android
export ANDROID_NDK_HOME=$HOME/Android/ndk/25.2.9519653
cd android
./gradlew assembleRelease

# === ALL (full workspace) ===
cargo build --release
cargo test --release
```

# AndroidCraft

[![Status](https://img.shields.io/badge/status-Extremely%20Beta-orange?style=flat-square)](https://github.com/irulune/AndroidCraft)
[![Platform](https://img.shields.io/badge/platform-Android-3DDC84?style=flat-square&logo=android&logoColor=white)](https://github.com/irulune/AndroidCraft)

An Android fork of [EffectCraft](https://github.com/storytold/effectcraft), bringing its After Effects-inspired editor to Android phones and tablets.

> [!WARNING]
> **AndroidCraft is extremely beta.**
>
> Expect crashes, incomplete features, broken layouts, performance issues, and other problems.

## About

EffectCraft is an open-source After Effects-inspired editor built around a Rust engine.

AndroidCraft is a fork of EffectCraft that focuses on getting that editor running on **Android**.

## Building

### Requirements

You'll need a Linux environment with:

- Git
- Rust / Cargo
- Java
- Android SDK
- Android NDK
- `cargo-apk`

### 1. Clone EffectCraft

```bash
mkdir -p ~/workingdir
cd ~/workingdir

git clone https://github.com/storytold/effectcraft.git ec
cd ec
```

### 2. Checkout the compatible revision

AndroidCraft currently uses:

`549e9d825132986d6167c1432cd3328efcbb0278`

```bash
git fetch --depth 1 origin \
  549e9d825132986d6167c1432cd3328efcbb0278

git checkout \
  549e9d825132986d6167c1432cd3328efcbb0278
```

### 3. Add AndroidCraft

```bash
cp -r ~/workingdir/build/crates/ui-mobile crates/
cp -r ~/workingdir/build/android .
```

### 4. Test the mobile UI

```bash
cargo test -p effectcraft-ui-mobile
```

### 5. Install cargo-apk

```bash
. ~/.cargo/env
cargo install cargo-apk --locked
```

### 6. Create a debug keystore

```bash
cd ~/workingdir/ec/android/app

keytool -genkey -v \
  -keystore debug.keystore \
  -storepass android \
  -alias androiddebugkey \
  -keypass android \
  -keyalg RSA \
  -keysize 2048 \
  -validity 10000 \
  -dname "CN=Android Debug,O=Android,C=US"
```

### 7. Build the APK

```bash
. ~/.cargo/env

cargo apk build \
  --release \
  --target aarch64-linux-android
```

### 8. Find the APK

```bash
find ~/workingdir/ec \
  -type f \
  -name '*.apk' \
  -print
```

## Compatibility

AndroidCraft is currently focused on **ARM64 Android devices**, including phones and tablets.

Device compatibility has not been extensively tested yet.

## Issues

If something doesn't work, open an issue and include:

- Device model
- Android version
- AndroidCraft commit/build
- Steps to reproduce
- Expected behavior
- Actual behavior
- Logs or screenshots

[Open an issue](https://github.com/irulune/AndroidCraft/issues)

## Upstream

AndroidCraft is based on [EffectCraft](https://github.com/storytold/effectcraft).

EffectCraft provides the editor and Rust engine that AndroidCraft currently builds around.

## License

AndroidCraft is derived from EffectCraft.

See the upstream repository and included license files for the applicable licensing and attribution requirements.

[EffectCraft](https://github.com/storytold/effectcraft)

Adobe and After Effects are trademarks of Adobe Inc. AndroidCraft is an independent project and is not affiliated with, sponsored by, or endorsed by Adobe.

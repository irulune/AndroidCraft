

An Android fork of "EffectCraft" (https://github.com/storytold/effectcraft), bringing its After Effects-inspired editor to Android phones and tablets.

""Status" (https://img.shields.io/badge/status-Extremely%20Beta-orange?style=flat-square)" (https://github.com/irulune/AndroidCraft)
""Platform" (https://img.shields.io/badge/platform-Android-3DDC84?style=flat-square&logo=android&logoColor=white)" (https://github.com/irulune/AndroidCraft)
""Rust" (https://img.shields.io/badge/built%20with-Rust-000000?style=flat-square&logo=rust&logoColor=white)" (https://www.rust-lang.org/)
""Issues" (https://img.shields.io/github/issues/irulune/AndroidCraft?style=flat-square)" (https://github.com/irulune/AndroidCraft/issues)
""Stars" (https://img.shields.io/github/stars/irulune/AndroidCraft?style=flat-square)" (https://github.com/irulune/AndroidCraft/stargazers)

«[!WARNING]
AndroidCraft is extremely beta.

It's still being worked on, so expect crashes, incomplete features, broken layouts, performance issues, and other problems. It isn't ready for production use yet.»

About

EffectCraft is an open-source After Effects-inspired editor built around a Rust engine.

AndroidCraft is a fork of EffectCraft that focuses on getting that editor running on Android.

The main difference is the target platform:

| EffectCraft| AndroidCraft
Desktop| ✓| —
Android| —| ✓
Phones| —| ✓
Tablets| —| ✓
ARM64| —| ✓

AndroidCraft is currently focused on making the existing editor work well enough on Android. It's not intended to maintain complete feature parity with the desktop version at this point.

Project structure

Most of the Android-specific work currently lives in two places:

crates/
└── ui-mobile/       # Mobile UI and editor interface

android/
└── app/             # Android application

The rest of the editor and engine comes from the EffectCraft source tree.

Building

The current build process starts with a specific EffectCraft revision and then adds the AndroidCraft files.

Requirements

You'll need a Linux environment with:

- Git
- Rust / Cargo
- Java
- Android SDK
- Android NDK
- "cargo-apk"

1. Clone EffectCraft

mkdir -p ~/workingdir
cd ~/workingdir

git clone https://github.com/storytold/effectcraft.git ec
cd ec

2. Checkout the compatible revision

AndroidCraft currently uses this EffectCraft commit:

549e9d825132986d6167c1432cd3328efcbb0278

git fetch --depth 1 origin \
  549e9d825132986d6167c1432cd3328efcbb0278

git checkout \
  549e9d825132986d6167c1432cd3328efcbb0278

You can check the current commit with:

git log --oneline -1

3. Add AndroidCraft

Copy the mobile UI:

cp -r ~/workingdir/build/crates/ui-mobile crates/

Then copy the Android project:

cp -r ~/workingdir/build/android .

The resulting tree should contain:

ec/
├── crates/
│   └── ui-mobile/
└── android/
    └── app/

4. Test the mobile UI

From the EffectCraft root:

cargo test -p effectcraft-ui-mobile

5. Install "cargo-apk"

. ~/.cargo/env
cargo install cargo-apk --locked

Check that it installed:

cargo apk --version

6. Create a debug keystore

Move into the Android app directory:

cd ~/workingdir/ec/android/app

Generate a debug keystore:

keytool -genkey -v \
  -keystore debug.keystore \
  -storepass android \
  -alias androiddebugkey \
  -keypass android \
  -keyalg RSA \
  -keysize 2048 \
  -validity 10000 \
  -dname "CN=Android Debug,O=Android,C=US"

Verify it exists:

ls -la debug.keystore

This is only for development. A proper release build should use its own release signing key.

7. Build the APK

From "android/app":

. ~/.cargo/env

cargo apk build \
  --release \
  --target aarch64-linux-android

The current build targets "aarch64-linux-android", so the resulting APK is intended for ARM64 Android devices.

8. Find the APK

Once the build finishes:

find ~/workingdir/ec \
  -type f \
  -name '*.apk' \
  -print

You can transfer the resulting APK to your Android device and install it normally.

With ADB:

adb install -r /path/to/androidcraft.apk

Building in a sandbox

The project has also been built successfully in a sandboxed Linux environment.

The sandbox isn't part of AndroidCraft itself; it is simply one environment that can provide the tools needed to compile the project.

If you're using another environment, the important parts are still the same:

EffectCraft
    ↓
checkout compatible commit
    ↓
add AndroidCraft
    ↓
Rust + Android toolchain
    ↓
cargo-apk
    ↓
AArch64 APK

Compatibility

AndroidCraft is currently being developed around ARM64 Android devices.

It's primarily intended for:

- Android phones
- Android tablets

Device compatibility hasn't been tested extensively yet, so behavior can vary between devices.

If you get it running on a device, feel free to report whether it works and what device you tested it on.

Issues

If you run into a problem, open an issue on GitHub.

"Open an issue →" (https://github.com/irulune/AndroidCraft/issues/new)

Useful information to include:

Device:
Android version:
AndroidCraft commit/build:

What happened:

Steps to reproduce:

Expected behavior:

Actual behavior:

Logs/screenshots:

Upstream

AndroidCraft is based on "EffectCraft" (https://github.com/storytold/effectcraft).

EffectCraft provides the editor and Rust engine that AndroidCraft is currently built around.

License

AndroidCraft is derived from EffectCraft.

See the upstream repository and the license files included with the source for the applicable licensing and attribution requirements.

"EffectCraft" (https://github.com/storytold/effectcraft)

Adobe and After Effects are trademarks of Adobe Inc. AndroidCraft is an independent project and is not affiliated with, sponsored by, or endorsed by Adobe.

---

AndroidCraft is still extremely beta. If it works, cool. If it doesn't, that's probably a bug.

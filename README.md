
<p align="center">
  <h1 align="center">AndroidCraft</h1>
  <p align="center">
    <b>After Effects-inspired motion graphics, brought to Android.</b>
  </p>
  <p align="center">
    An experimental Android fork of EffectCraft for phones and tablets.
  </p>
</p>«[!WARNING]
AndroidCraft is Extremely Beta (EB).

This project is very early in development. Expect incomplete features, crashes, performance issues, rough touch interactions, and breaking changes. It is currently an experimental project and is not intended for production use.»

---

What is AndroidCraft?

AndroidCraft is an Android-focused fork of "EffectCraft" (https://github.com/storytold/effectcraft), an open-source motion graphics and visual effects editor inspired by Adobe After Effects.

EffectCraft and Adobe After Effects are primarily designed for desktop environments.

AndroidCraft takes the project in a different direction:

«What if an After Effects-style editor could run on a phone or tablet?»

The project is an experiment in bringing motion graphics, compositing, animation, and visual-effects workflows to Android devices.

Platforms

Platform| Status
Android phones| 🧪 Extremely Beta
Android tablets| 🧪 Extremely Beta
Windows| ❌
macOS| ❌
Linux| ❌

AndroidCraft is specifically focused on Android. The desktop versions of EffectCraft are separate from this project.

---

Relationship to EffectCraft

AndroidCraft is a fork of EffectCraft.

EffectCraft is an open-source project that recreates the general workflow and functionality of Adobe After Effects through a clean-room implementation.

AndroidCraft uses EffectCraft as its upstream foundation and adds the Android/mobile side of the project.

In simple terms

Adobe After Effects

↓ inspires

EffectCraft

↓ forked and adapted for Android

AndroidCraft

AndroidCraft is therefore not an official Adobe application and is not affiliated with, sponsored by, or endorsed by Adobe.

---

Current Status

AndroidCraft is currently Extremely Beta (EB).

The project is still being actively developed, and the Android experience is far from finished.

Things that may currently be rough include:

- Touch controls
- Mobile UI layout
- Performance
- Device compatibility
- Rendering
- Memory usage
- Missing or incomplete features
- Android-specific behavior
- Stability

The fact that an APK successfully builds does not mean that every feature of EffectCraft works correctly on Android.

If something breaks, that's expected at this stage. 🗿

---

Building AndroidCraft

AndroidCraft currently uses the EffectCraft source tree as its build foundation.

The Android build can be performed in a Linux-based development environment such as the environment used during development.

Requirements

You will need:

- Git
- Rust
- Cargo
- Java / "keytool"
- Android SDK
- Android NDK
- Android "aarch64" toolchain
- "cargo-apk"

The exact environment may vary depending on where you build AndroidCraft.

---

1. Set up Rust

Load Cargo's environment:

. ~/.cargo/env

Check that Rust is available:

cargo --version

---

2. Clone EffectCraft

Create the working directory:

mkdir -p ~/workingdir
cd ~/workingdir

Clone EffectCraft:

git clone https://github.com/storytold/effectcraft.git ec

Enter the repository:

cd ec

---

3. Checkout the compatible EffectCraft revision

AndroidCraft currently builds against this EffectCraft revision:

549e9d825132986d6167c1432cd3328efcbb0278

Fetch it:

git fetch --depth 1 origin \
  549e9d825132986d6167c1432cd3328efcbb0278

Then checkout the revision:

git checkout \
  549e9d825132986d6167c1432cd3328efcbb0278

You can verify it with:

git log --oneline -1

---

4. Add AndroidCraft's mobile code

AndroidCraft's development files are placed into the EffectCraft source tree.

From the EffectCraft root:

cp -r ~/workingdir/build/crates/ui-mobile crates/
cp -r ~/workingdir/build/android .

The resulting project contains:

ec/
├── crates/
│   └── ui-mobile/
│
└── android/
    └── app/

The mobile UI is contained in:

crates/ui-mobile

The Android application is contained in:

android/app

---

5. Test the mobile UI

From the EffectCraft root:

cargo test -p effectcraft-ui-mobile

This checks the AndroidCraft mobile UI crate before attempting the Android build.

---

6. Install "cargo-apk"

Install the Android packaging tool:

. ~/.cargo/env
cargo install cargo-apk --locked

You can verify that it installed correctly with:

cargo apk --version

---

7. Create a debug keystore

For local Android development, a debug keystore can be generated inside the Android app directory:

cd ~/workingdir/ec/android/app

Then:

keytool -genkey -v \
  -keystore debug.keystore \
  -storepass android \
  -alias androiddebugkey \
  -keypass android \
  -keyalg RSA \
  -keysize 2048 \
  -validity 10000 \
  -dname "CN=Android Debug,O=Android,C=US"

Verify that it exists:

ls -la debug.keystore

«[!NOTE]
This is a development/debug keystore. Do not use a debug key as the signing key for a production release.»

---

8. Build the APK

From:

~/workingdir/ec/android/app

run:

. ~/.cargo/env

cargo apk build \
  --release \
  --target aarch64-linux-android

The "aarch64-linux-android" target builds for 64-bit ARM Android devices, which covers most modern Android phones and tablets.

The first build can take significantly longer because Rust needs to compile the project and its dependencies.

---

9. Find the APK

After the build completes:

find ~/workingdir/ec \
  -type f \
  -name '*.apk' \
  -print

This will show the generated APK.

You can then transfer the APK to your Android device and install it.

If you have ADB available:

adb install -r /path/to/androidcraft.apk

---

Development Build Flow

The overall process is:

AndroidCraft
     │
     ├── crates/ui-mobile
     │
     └── android/
          │
          ▼
     EffectCraft source
          │
          ▼
     Rust + cargo-apk
          │
          ▼
     Android AArch64 build
          │
          ▼
     APK
          │
          ▼
   Android phone/tablet

---

Why AndroidCraft?

Desktop motion-graphics software can be powerful, but it also requires a desktop computer.

AndroidCraft explores whether that experience can be brought to hardware that people already carry with them.

The long-term idea is simple:

Motion graphics shouldn't necessarily be limited to the desktop.

---

Contributing

AndroidCraft is still extremely early, so feedback is especially useful.

When reporting a problem, include:

- Android device model
- Android version
- What you were doing
- Steps to reproduce the issue
- Expected behavior
- Actual behavior
- Screenshots or recordings
- Relevant logs

Report problems through "GitHub Issues" (https://github.com/irulune/AndroidCraft/issues).

---

Credits

AndroidCraft is based on "EffectCraft" (https://github.com/storytold/effectcraft).

Huge thanks to the EffectCraft contributors for building the upstream project that makes this Android experiment possible.

AndroidCraft is an independent project and does not represent Adobe.

Trademarks

Adobe and After Effects are trademarks or registered trademarks of Adobe Inc. in the United States and/or other countries.

AndroidCraft and EffectCraft are independent open-source projects and are not affiliated with, sponsored by, or endorsed by Adobe Inc.

---

License

AndroidCraft is derived from EffectCraft.

Please refer to the upstream EffectCraft repository and the license files included with the source for the applicable licensing terms and attribution requirements.

"EffectCraft on GitHub" (https://github.com/storytold/effectcraft)

---

<p align="center">
  <b>AndroidCraft</b><br>
  <sub>Motion graphics beyond the desktop.</sub>
</p>

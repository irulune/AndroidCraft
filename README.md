# AndroidCraft

A dark touch editor for phones and tablets, built on the EffectCraft Rust engine (https://github.com/storytold/effectcraft).

- crates/ui-mobile: effectcraft-ui-mobile (touch UI, layout, editor facade, operations)
- android/app: Android entry point (NativeActivity + eframe)

Build: check out EffectCraft at 549e9d825132986d6167c1432cd3328efcbb0278, copy crates/ui-mobile into crates/ and android/ into the root, then cargo test -p effectcraft-ui-mobile and, in android/app, cargo apk build --target aarch64-linux-android.

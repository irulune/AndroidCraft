# Effectia

A dark, touch-first motion graphics editor for Android phones and tablets, built on the
[EffectCraft](https://github.com/storytold/effectcraft) Rust engine. Everything on screen is an
engine command, so the app and the engine always agree about what a project contains.

- `crates/ui-mobile`: `effectia-ui-mobile`, the touch UI, responsive layouts, editor facade,
  operations and tests (egui)
- `android/app`: the Android entry point (NativeActivity + eframe), package `app.effectia`

## What it does
Layers, a timeline with keyframes, a property panel, effects, compositions, PNG / MP4 / GIF export
(renders show progress and can be stopped), **Track** (3D camera solve and subject isolation from a
clip), and external-keyboard shortcuts from the engine's own table.

- **Tap to select**: tap the picture to pick the layer under your finger (tap again to cycle
  through the stack); drag it to move. A Move / Select switch keeps taps from moving things.
- **Masks and shape paths**: add a rectangle or ellipse mask from the layer menu; mode (Add,
  Subtract, Intersect, Lighten, Darken, Difference), invert and remove from the mask's header.
  *Edit points* works on masks and shape paths: drag points and bezier handles on the picture, tap
  an edge to add a point, and use Delete point, Make smooth / Make corner and Open / Close path. A
  drag is one undo step.
- **Gradients**: *Add gradient fill / stroke* from the layer menu; a gradient property opens a stop
  editor (add, move and delete colour and opacity stops; colour and opacity per stop).
- **Layer links**: a layer-reference property picks a layer from a list, and *Link to another
  property* (pick whip) builds the expression from a two-step layer then property list.
- **Expressions**: a property's name menu adds, edits or removes its expression. One-tap starters
  (wiggle, loopOut, time-driven, sine) help on a phone keyboard; a syntax error is refused with the
  engine's message and the previous expression stays.
- **Curve editor**: select a keyframe, tap Curve, and drag the outgoing and incoming handles of
  the segment (overshoot allowed), or pick Easy Ease, Linear, Ease in or Ease out. A drag is one
  undo step.
- **Media picker** (Add > Photos, videos and files): recent photos, videos and audio as a grid
  (audio shows its length), an All / Photos / Videos / Audio filter, folder browsing, multi-select.
  It asks for photo, video and audio access; optional "All files access"
  (`MANAGE_EXTERNAL_STORAGE`, opened in system settings) unlocks other file types. That permission
  is restricted on Google Play, which matters only for a Play release.
- **Project gallery** (header logo, File > Projects): open, rename, duplicate, delete, thumbnails.
  It is self-contained in `src/gallery.rs` and `src/ui/gallery.rs`, so a custom projects page can
  replace it.

### Performance
Frames are drawn on a background thread (`src/renderer.rs`), so touches and scrolling never wait on
a render; the last frame stays on screen until the next lands. While you scrub, play or drag, draft
frames are sized to the speed the engine is actually drawing at, and the full-quality frame follows
when you stop. On a desktop release build the interface costs about 1 ms a frame while scrubbing,
against 57 to 220 ms when frames were drawn on the UI thread. Measure it with
`cargo run --release -p effectia-ui-mobile --example perf`.

### Resizable windows
The activity is declared resizable (`resizeableActivity`) and handles every configuration change, so
split-screen, freeform windows and foldables never restart it, and the layout re-flows live.
Drag the timeline's top strip to change its height and, on wide windows, the properties panel's edge
to change its width; sizes are remembered between runs. When the window becomes too narrow to dock
the properties panel it closes instead of covering the picture, and returns when the window grows.

## Build
Effectia is two folders added to an EffectCraft checkout.

1. Check out EffectCraft at `549e9d825132986d6167c1432cd3328efcbb0278`.
2. Copy `crates/ui-mobile` into its `crates/` folder and `android/` into its root.
3. `cargo test -p effectia-ui-mobile`
4. Android SDK and NDK, `rustup target add aarch64-linux-android`, `cargo install cargo-apk`.
5. `cd android/app && cargo apk build --release`

The release profile signs with `debug.keystore` (see `android/app/Cargo.toml`). Create that keystore
(`keytool -genkey -v -keystore debug.keystore -storepass android -alias androiddebugkey -keypass
android -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug"`), or point
`[package.metadata.android.signing.release]` at your own, before building.

## Known limits
- Nothing here has been run on a physical device: permission prompts (requested over JNI from the
  app thread, since NativeActivity has no result callback), the all-files settings page, safe-area
  handling, split-screen resizing and the soft keyboard are all unverified, and the Android entry
  crate has not been compiled for an Android target in the environment that wrote it.
- Resizing is tested as layouts and drags in a headless harness, and the activity declares no
  minimum window size.
- No share sheet for exports (needs a FileProvider and Java glue).
- Importing many large videos at once blocks the UI while they are added.

Licensed MIT or Apache-2.0, like EffectCraft.

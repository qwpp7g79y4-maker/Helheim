# helheim-math-plugin

Minimal example native module for the Helheim FFI / Dynamic Module System.

## Build

From the repository root:

```bash
cargo build -p helheim-math-plugin --release
```

The resulting library will be at:

- `target/release/libhelheim_math_plugin.so` (Linux)
- `target/release/libhelheim_math_plugin.dylib` (macOS)
- `target/release/helheim_math_plugin.dll` (Windows)

## Usage in Helheim

Copy/rename/symlink the library so the `NativeModuleLoader` can find it as "math", for example:

```bash
cp target/release/libhelheim_math_plugin.so ~/.helheim/lib/libmath.so
```

Then in a `.hel` script:

```hel
gebruik "math"

zet pi = roep_aan math::pi
zet s = roep_aan math::sin 1.5708
zet totaal = roep_aan math::add 40 2

druk_af pi
druk_af s
druk_af totaal
```

## Notes

- This plugin demonstrates correct use of the HelValue union, HelFFIContext, and error reporting via `report_error`.
- All complex returns (if any) would be allocated with the context allocator.
- The function table is provided via `helheim_get_function_table`.

This is intended as a reference implementation while Antigravity wires the loader into MemoryManager + Executor dispatch.
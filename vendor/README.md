# Local dependency patch

`tray-icon` is the MIT/Apache-2.0 upstream crate version 0.21.3, copied from the Cargo registry sources with its license files. Source: [upstream repository](https://github.com/tauri-apps/tray-icon).

The upstream source changes set `NOTIFYICONDATAW.cbSize` in the Windows registration, icon/tooltip modification and removal requests. Without the required structure size, the Shell can reject those requests. Initial registration failure now destroys the owned host window and returns the Windows error rather than returning a successful builder result. This prevents the application from hiding its only usable window when Shell access is denied. Removal errors also include the native error text.

The native application diagnostic requires a Shell-reported icon rectangle as well as successful cleanup. This environment denies registration with Windows error 5 even with valid parameters; that is recorded as a blocked native tray test. Keep this patch until a verified upstream version supplies these fixes. No shared Cargo cache files are modified.

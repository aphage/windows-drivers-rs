# windows-drivers-rs


This repo is a collection of Rust crates that enable developers to develop Windows Drivers in Rust. It is the intention to support both WDM and WDF driver development models. This repo contains the following crates:

* [wdk-build](./crates/wdk-build): A library to configure a Cargo build script for binding generation and downstream linking of the WDK (Windows Developer Kit). While this crate is written to be flexible with different WDK releases and different WDF version, it is currently only tested for NI eWDK, KMDF 1.33, UMDF 2.33, and WDM Drivers. There may be missing linker options for older DDKs.
* [wdk-sys](./crates/wdk-sys): Direct FFI bindings to APIs available in the Windows Development Kit (WDK). This includes both autogenerated ffi bindings from `bindgen`, and also manual re-implementations of macros that bindgen fails to generate.
* [wdk](./crates/wdk): Safe idiomatic bindings to APIs available in the Windows Development Kit (WDK)
* [wdk-panic](./crates/wdk-panic/): Default panic handler implementations for programs built with WDK
* [wdk-alloc](./crates/wdk-alloc): alloc support for binaries compiled with the Windows Development Kit (WDK)
* [wdk-macros](./crates/wdk-macros): A collection of macros that help make it easier to interact with wdk-sys's direct bindings. This crate is re-exported via `wdk-sys` and crates should typically never need to directly depend on `wdk-macros`

To see an example of this repo used to create drivers, see [Windows-rust-driver-samples](https://github.com/microsoft/Windows-rust-driver-samples).

Note: This project is still in early stages of development and is not yet recommended for commercial use. We encourage community experimentation, suggestions and discussions! We will be using our [GitHub Discussions forum](https://github.com/microsoft/windows-drivers-rs/discussions) as the main form of engagement with the community!

## <a name="supported-configs">Supported Configurations

This project was built with support of WDM, KMDF, and UMDF drivers in mind, as well as Win32 Services. This includes support for all versions of WDF included in WDK 22H2 and newer. Currently, the crates available on [`crates.io`](https://crates.io) only support KMDF v1.33, but bindings can be generated for everything else by cloning `windows-drivers-rs` and modifying the config specified in [`build.rs` of `wdk-sys`](./crates/wdk-sys/build.rs). Crates.io support for other WDK configurations is planned in the near future.

## Getting Started

### Build Requirements

* Binding generation via `bindgen` requires `libclang`. The easiest way to acquire this is via `winget`
  * `winget install LLVM.LLVM`
* To execute post-build tasks (ie. `inf2cat`, `infverif`, etc.), `cargo make` is used
  * `cargo install --locked cargo-make --no-default-features --features tls-native`

* Building programs with the WDK also requires being in a valid WDK environment. The recommended way to do this is to [enter an eWDK developer prompt](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk#getting-started)

## Adding windows-drivers-rs to Your Driver Package

The crates in this repository are available from [`crates.io`](https://crates.io), but take into account the current limitations outlined in [Supported Configurations](#supported-configs). If you need to support a different config, try cloning this repo and using [path dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-path-dependencies)

1. Create a new Cargo package with a lib crate:

   ```pwsh
   cargo new <driver_name> --lib --config
   ```

2. Add dependencies on `windows-drivers-rs` crates:

   ```pwsh
   cd <driver_name>
   cargo add --build wdk-build
   cargo add wdk wdk-sys wdk-alloc wdk-panic
   ```

3. Set the crate type to `cdylib` by adding the following snippet to `Cargo.toml`:

   ```toml
   [lib]
   crate-type = ["cdylib"]
   ```

4. Set crate panic strategy to `abort` in `Cargo.toml`:

   ```toml
   [profile.dev]
   panic = "abort"
   lto = true # optional setting to enable Link Time Optimizations

   [profile.release]
   panic = "abort"
   lto = true # optional setting to enable Link Time Optimizations
   ```

5. Create a `build.rs` and add the following snippet:

   ```rust
   fn main() -> Result<(), wdk_build::ConfigError> {
      wdk_build::Config::from_env_auto()?.configure_binary_build();
      Ok(())
   }
   ```

6. Mark your driver as `no_std` in `lib.rs`:

   ```rust
   #![no_std]
   ```

7. Add a panic handler in `lib.rs`:

   ```rust
   #[cfg(not(test))]
   extern crate wdk_panic;

   ```

8. Add a global allocator in `lib.rs`:

   ```rust
   #[cfg(not(test))]
   use wdk_alloc::WDKAllocator;

   #[cfg(not(test))]
   #[global_allocator]
   static GLOBAL_ALLOCATOR: WDKAllocator = WDKAllocator;
   ```

9. Add a DriverEntry in `lib.rs`:

   ```rust
   use wdk_sys::{
      DRIVER_OBJECT,
      NTSTATUS,
      PCUNICODE_STRING,
   };

   #[export_name = "DriverEntry"] // WDF expects a symbol with the name DriverEntry
   pub unsafe extern "system" fn driver_entry(
      driver: &mut DRIVER_OBJECT,
      registry_path: PCUNICODE_STRING,
   ) -> NTSTATUS {
      0
   }
   ```

10. Add a `Makefile.toml`:

   ```toml
   extend = ".cargo-make-loadscripts/rust-driver-makefile.toml"

   [env]
   CARGO_MAKE_EXTEND_WORKSPACE_MAKEFILE = true

   [config]
   load_script = """
   pwsh.exe -Command "\
   if ($env:CARGO_MAKE_CRATE_IS_WORKSPACE) { return };\
   $cargoMakeURI = 'https://raw.githubusercontent.com/microsoft/windows-drivers-rs/main/rust-driver-makefile.toml';\
   New-Item -ItemType Directory .cargo-make-loadscripts -Force;\
   Invoke-RestMethod -Method GET -Uri $CargoMakeURI -OutFile $env:CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY/.cargo-make-loadscripts/rust-driver-makefile.toml\
   "
   """
   ```

11. Add an inx file that matches the name of your `cdylib` crate.

12. Build the driver:

   ```pwsh
   cargo make
   ```

A `DriverCertificate.cer` file will be generated, and a signed driver package will be available at `target/<Cargo profile>/package`

## Trademark Notice

Trademarks This project may contain trademarks or logos for projects, products, or services. Authorized use of Microsoft trademarks or logos is subject to and must follow Microsoft’s Trademark & Brand Guidelines. Use of Microsoft trademarks or logos in modified versions of this project must not cause confusion or imply Microsoft sponsorship. Any use of third-party trademarks or logos are subject to those third-party’s policies.
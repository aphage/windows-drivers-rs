// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![cfg_attr(nightly_toolchain, feature(assert_matches))]
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]

mod bindgen;
mod utils;

use std::{env, path::PathBuf};

pub use bindgen::BuilderExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utils::PathExt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub wdk_content_root: PathBuf,
    pub driver_config: DriverConfig,
    pub cpu_architecture: CPUArchitecture,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DriverConfig {
    WDM(),
    KMDFConfig(KMDFConfig),
    UMDFConfig(UMDFConfig),
}

#[derive(Debug, Clone, Copy)]
pub enum DriverType {
    /// Windows Driver Model
    WDM,
    /// Kernel Mode Driver Framework
    KMDFConfig,
    /// User Mode Driver Framework
    UMDFConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CPUArchitecture {
    AMD64,
    ARM64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct KMDFConfig {
    pub kmdf_version_major: u8,
    pub kmdf_version_minor: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct UMDFConfig {
    pub umdf_version_major: u8,
    pub umdf_version_minor: u8,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("cannot find directory: {directory}")]
    DirectoryNotFound { directory: String },
    #[error(transparent)]
    StripExtendedPathPrefixError(#[from] utils::StripExtendedPathPrefixError),
    #[error(transparent)]
    ConfigFromEnvError(#[from] ConfigFromEnvError),
    #[error(transparent)]
    ExportError(#[from] ExportError),
}

#[derive(Debug, Error)]
pub enum ConfigFromEnvError {
    #[error(transparent)]
    EnvError(#[from] std::env::VarError),
    #[error(transparent)]
    DeserializeError(#[from] serde_json::Error),
    #[error(
        "config from {config_1_source} does not match config from {config_2_source}:\nconfig_1: \
         {config_1:?}\nconfig_2: {config_2:?}"
    )]
    ConfigMismatch {
        config_1: Box<Config>,
        config_1_source: String,
        config_2: Box<Config>,
        config_2_source: String,
    },
    #[error("no WDK configs exported from dependencies could be found")]
    ConfigNotFound,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(
        "Missing `links` value in crate's config.toml. Metadata is unable to propogate to \
         dependencies without a `links` value"
    )]
    MissingLinksValue(#[from] std::env::VarError),
    #[error(transparent)]
    SerializeError(#[from] serde_json::Error),
}

impl Default for Config {
    #[must_use]
    fn default() -> Self {
        Self {
            wdk_content_root: utils::detect_wdk_content_root().expect(
                "WDKContentRoot should be able to be detected. Ensure that the WDK is installed, \
                 or that the environment setup scripts in the eWDK have been run.",
            ),
            driver_config: DriverConfig::WDM(),
            cpu_architecture: utils::detect_cpu_architecture_in_build_script(),
        }
    }
}

impl Config {
    const CARGO_CONFIG_KEY: &'static str = "wdk_config";

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a [`Config`] from a config exported from a dependency. The
    /// dependency must have exported a [`Config`] via
    /// [`Config::export_config`], and the dependency must have set a `links`
    /// value in its Cargo manifiest to export the
    /// `DEP_<CARGO_MANIFEST_LINKS>_WDK_CONFIG` to downstream crates
    ///
    /// # Errors
    ///
    /// This function will return an error if the provided `links_value` does
    /// not correspond with a links value specified in the Cargo
    /// manifest of one of the dependencies of the crate who's build script
    /// invoked this function.
    pub fn from_env<S: AsRef<str> + std::fmt::Display>(
        links_value: S,
    ) -> Result<Self, ConfigFromEnvError> {
        Ok(serde_json::from_str::<Self>(
            std::env::var(format!(
                "DEP_{links_value}_{}",
                Self::CARGO_CONFIG_KEY.to_ascii_uppercase()
            ))?
            .as_str(),
        )?)
    }

    /// Creates a [`Config`] from a config exported from [`wdk`](https://docs.rs/wdk/latest/wdk/) or
    /// [`wdk_sys`](https://docs.rs/wdk-sys/latest/wdk_sys/) crates.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    ///     * an exported config from a dependency on [`wdk`](https://docs.rs/wdk/latest/wdk/)
    ///       and/or [`wdk_sys`](https://docs.rs/wdk-sys/latest/wdk_sys/) cannot
    ///       be found
    ///     * there is a config mismatch between [`wdk`](https://docs.rs/wdk/latest/wdk/)
    ///       and [`wdk_sys`](https://docs.rs/wdk-sys/latest/wdk_sys/)
    pub fn from_env_auto() -> Result<Self, ConfigFromEnvError> {
        let wdk_sys_crate_dep_key =
            format!("DEP_WDK_{}", Self::CARGO_CONFIG_KEY.to_ascii_uppercase());
        let wdk_crate_dep_key = format!(
            "DEP_WDK-SYS_{}",
            Self::CARGO_CONFIG_KEY.to_ascii_uppercase()
        );

        let wdk_sys_crate_config_serialized = std::env::var(&wdk_sys_crate_dep_key);
        let wdk_crate_config_serialized = std::env::var(&wdk_crate_dep_key);

        if let (Ok(wdk_sys_crate_config_serialized), Ok(wdk_crate_config_serialized)) = (
            wdk_sys_crate_config_serialized.clone(),
            wdk_crate_config_serialized.clone(),
        ) {
            let wdk_sys_crate_config =
                serde_json::from_str::<Self>(&wdk_sys_crate_config_serialized)?;
            let wdk_crate_config = serde_json::from_str::<Self>(&wdk_crate_config_serialized)?;

            if wdk_sys_crate_config == wdk_crate_config {
                Ok(wdk_sys_crate_config)
            } else {
                Err(ConfigFromEnvError::ConfigMismatch {
                    config_1: Box::from(wdk_sys_crate_config),
                    config_1_source: wdk_sys_crate_dep_key,
                    config_2: Box::from(wdk_crate_config),
                    config_2_source: wdk_crate_dep_key,
                })
            }
        } else if let Ok(wdk_sys_crate_config_serialized) = wdk_sys_crate_config_serialized {
            Ok(serde_json::from_str::<Self>(
                &wdk_sys_crate_config_serialized,
            )?)
        } else if let Ok(wdk_crate_config_serialized) = wdk_crate_config_serialized {
            Ok(serde_json::from_str::<Self>(&wdk_crate_config_serialized)?)
        } else {
            Err(ConfigFromEnvError::ConfigNotFound)
        }
    }

    /// Returns header include paths required to build and link based off of the
    /// configuration of `Config`
    ///
    /// # Errors
    ///
    /// This function will return an error if any of the required paths do not
    /// exist.
    pub fn get_include_paths(&self) -> Result<Vec<PathBuf>, ConfigError> {
        let mut include_paths = vec![];

        let include_directory = self.wdk_content_root.join("Include");

        // Add windows sdk include paths
        // Based off of logic from WindowsDriver.KernelMode.props &
        // WindowsDriver.UserMode.props in NI(22H2) WDK
        let sdk_version = utils::get_latest_windows_sdk_version(include_directory.as_path())?;
        let windows_sdk_include_path = include_directory.join(sdk_version);

        let crt_include_path = windows_sdk_include_path.join("km/crt");
        if !crt_include_path.is_dir() {
            return Err(ConfigError::DirectoryNotFound {
                directory: crt_include_path.to_string_lossy().into(),
            });
        }
        include_paths.push(
            crt_include_path
                .canonicalize()?
                .strip_extended_length_path_prefix()?,
        );

        let km_or_um_include_path = windows_sdk_include_path.join(match self.driver_config {
            DriverConfig::WDM() | DriverConfig::KMDFConfig(_) => "km",
            DriverConfig::UMDFConfig(_) => "um",
        });
        if !km_or_um_include_path.is_dir() {
            return Err(ConfigError::DirectoryNotFound {
                directory: km_or_um_include_path.to_string_lossy().into(),
            });
        }
        include_paths.push(
            km_or_um_include_path
                .canonicalize()?
                .strip_extended_length_path_prefix()?,
        );

        let kit_shared_include_path = windows_sdk_include_path.join("shared");
        if !kit_shared_include_path.is_dir() {
            return Err(ConfigError::DirectoryNotFound {
                directory: kit_shared_include_path.to_string_lossy().into(),
            });
        }
        include_paths.push(
            kit_shared_include_path
                .canonicalize()?
                .strip_extended_length_path_prefix()?,
        );

        // Add other driver type-specific include paths
        match &self.driver_config {
            DriverConfig::WDM() => {}
            DriverConfig::KMDFConfig(kmdf_options) => {
                let kmdf_include_path = include_directory.join(format!(
                    "wdf/kmdf/{}.{}",
                    kmdf_options.kmdf_version_major, kmdf_options.kmdf_version_minor
                ));
                if !kmdf_include_path.is_dir() {
                    return Err(ConfigError::DirectoryNotFound {
                        directory: kmdf_include_path.to_string_lossy().into(),
                    });
                }
                include_paths.push(
                    kmdf_include_path
                        .canonicalize()?
                        .strip_extended_length_path_prefix()?,
                );
            }
            DriverConfig::UMDFConfig(umdf_options) => {
                let umdf_include_path = include_directory.join(format!(
                    "wdf/umdf/{}.{}",
                    umdf_options.umdf_version_major, umdf_options.umdf_version_minor
                ));
                if !umdf_include_path.is_dir() {
                    return Err(ConfigError::DirectoryNotFound {
                        directory: umdf_include_path.to_string_lossy().into(),
                    });
                }
                include_paths.push(
                    umdf_include_path
                        .canonicalize()?
                        .strip_extended_length_path_prefix()?,
                );
            }
        }

        Ok(include_paths)
    }

    /// Returns library include paths required to build and link based off of
    /// the configuration of `Config`
    ///
    /// # Errors
    ///
    /// This function will return an error if any of the required paths do not
    /// exist.
    pub fn get_library_paths(&self) -> Result<Vec<PathBuf>, ConfigError> {
        let mut library_paths = vec![];

        let library_directory = self.wdk_content_root.join("Lib");

        // Add windows sdk library paths
        // Based off of logic from WindowsDriver.KernelMode.props &
        // WindowsDriver.UserMode.props in NI(22H2) WDK
        let sdk_version = utils::get_latest_windows_sdk_version(library_directory.as_path())?;
        let windows_sdk_library_path =
            library_directory
                .join(sdk_version)
                .join(match self.driver_config {
                    DriverConfig::WDM() | DriverConfig::KMDFConfig(_) => {
                        format!("km/{}", self.cpu_architecture.to_windows_str(),)
                    }
                    DriverConfig::UMDFConfig(_) => {
                        format!("um/{}", self.cpu_architecture.to_windows_str(),)
                    }
                });
        if !windows_sdk_library_path.is_dir() {
            return Err(ConfigError::DirectoryNotFound {
                directory: windows_sdk_library_path.to_string_lossy().into(),
            });
        }
        library_paths.push(
            windows_sdk_library_path
                .canonicalize()?
                .strip_extended_length_path_prefix()?,
        );

        // Add other driver type-specific library paths
        match &self.driver_config {
            DriverConfig::WDM() => (),
            DriverConfig::KMDFConfig(kmdf_options) => {
                let kmdf_library_path = library_directory.join(format!(
                    "wdf/kmdf/{}/{}.{}",
                    self.cpu_architecture.to_windows_str(),
                    kmdf_options.kmdf_version_major,
                    kmdf_options.kmdf_version_minor
                ));
                if !kmdf_library_path.is_dir() {
                    return Err(ConfigError::DirectoryNotFound {
                        directory: kmdf_library_path.to_string_lossy().into(),
                    });
                }
                library_paths.push(
                    kmdf_library_path
                        .canonicalize()?
                        .strip_extended_length_path_prefix()?,
                );
            }
            DriverConfig::UMDFConfig(umdf_options) => {
                let umdf_library_path = library_directory.join(format!(
                    "wdf/umdf/{}/{}.{}",
                    self.cpu_architecture.to_windows_str(),
                    umdf_options.umdf_version_major,
                    umdf_options.umdf_version_minor
                ));
                if !umdf_library_path.is_dir() {
                    return Err(ConfigError::DirectoryNotFound {
                        directory: umdf_library_path.to_string_lossy().into(),
                    });
                }
                library_paths.push(
                    umdf_library_path
                        .canonicalize()?
                        .strip_extended_length_path_prefix()?,
                );
            }
        }

        Ok(library_paths)
    }

    /// Configures a Cargo build of a library that directly depends on the
    /// WDK (i.e. not transitively via wdk-sys). This emits specially
    /// formatted prints to Cargo based on this [`Config`].
    ///
    /// This includes header include paths, linker search paths, library link
    /// directives, and WDK-specific configuration definitions. This must be
    /// called from a Cargo build script of the library.
    ///
    /// # Errors
    ///
    /// This function will return an error if any of the required paths do not
    /// exist.
    ///
    /// # Panics
    ///
    /// Panics if the invoked from outside a Cargo build environment
    pub fn configure_library_build(&self) -> Result<(), ConfigError> {
        let library_paths = self.get_library_paths()?;

        // Emit linker search paths
        for path in library_paths {
            println!("cargo:rustc-link-search={}", path.display());
        }

        match &self.driver_config {
            DriverConfig::WDM() => {
                // Emit WDM-specific libraries to link to
                println!("cargo:rustc-link-lib=BufferOverflowFastFailK");
                println!("cargo:rustc-link-lib=ntoskrnl");
                println!("cargo:rustc-link-lib=hal");
                println!("cargo:rustc-link-lib=wmilib");
            }
            DriverConfig::KMDFConfig(_) => {
                // Emit KMDFConfig-specific libraries to link to
                println!("cargo:rustc-link-lib=BufferOverflowFastFailK");
                println!("cargo:rustc-link-lib=ntoskrnl");
                println!("cargo:rustc-link-lib=hal");
                println!("cargo:rustc-link-lib=wmilib");
                println!("cargo:rustc-link-lib=WdfLdr");
                println!("cargo:rustc-link-lib=WdfDriverEntry");
            }
            DriverConfig::UMDFConfig(umdf_options) => {
                // Emit UMDFConfig-specific libraries to link to
                match env::var("PROFILE")
                    .expect(
                        "Cargo should have set a valid PROFILE environment variable at build time",
                    )
                    .as_str()
                {
                    "release" => {
                        println!("cargo:rustc-link-lib=ucrt");
                    }
                    "debug" => {
                        println!("cargo:rustc-link-lib=ucrtd");
                    }
                    _ => {
                        unreachable!(r#"Cargo should always set a value of "release" or "debug""#);
                    }
                }

                if umdf_options.umdf_version_major >= 2 {
                    println!("cargo:rustc-link-lib=WdfDriverStubUm");
                    println!("cargo:rustc-link-lib=ntdll");
                }

                println!("cargo:rustc-link-lib=mincore");
            }
        }

        Ok(())
    }

    /// Configures a Cargo build of a binary that depends on the WDK. This
    /// emits specially formatted prints to Cargo based on this [`Config`].
    ///
    /// This consists mainly of linker setting configuration. This must be
    /// called from a Cargo build script of the binary being built
    pub fn configure_binary_build(&self) {
        // Linker arguments derived from Microsoft.Link.Common.props in Ni(22H2) WDK
        println!("cargo:rustc-cdylib-link-arg=/NXCOMPAT");
        println!("cargo:rustc-cdylib-link-arg=/DYNAMICBASE");

        // Always generate Map file with Exports
        println!("cargo:rustc-cdylib-link-arg=/MAP");
        println!("cargo:rustc-cdylib-link-arg=/MAPINFO:EXPORTS");

        // Force Linker Optimizations
        println!("cargo:rustc-cdylib-link-arg=/OPT:REF,ICF");

        // Enable "Forced Integrity Checking" to prevent non-signed binaries from
        // loading
        println!("cargo:rustc-cdylib-link-arg=/INTEGRITYCHECK");

        // Disable Manifest File Generation
        println!("cargo:rustc-cdylib-link-arg=/MANIFEST:NO");

        match &self.driver_config {
            DriverConfig::WDM() => {
                // Linker arguments derived from WindowsDriver.KernelMode.props in Ni(22H2) WDK
                println!("cargo:rustc-cdylib-link-arg=/DRIVER");
                println!("cargo:rustc-cdylib-link-arg=/NODEFAULTLIB");
                println!("cargo:rustc-cdylib-link-arg=/SUBSYSTEM:NATIVE");
                println!("cargo:rustc-cdylib-link-arg=/KERNEL");

                // Linker arguments derived from WindowsDriver.KernelMode.WDM.props in Ni(22H2)
                // WDK
                println!("cargo:rustc-cdylib-link-arg=/ENTRY:DriverEntry");
            }
            DriverConfig::KMDFConfig(_) => {
                // Linker arguments derived from WindowsDriver.KernelMode.props in Ni(22H2) WDK
                println!("cargo:rustc-cdylib-link-arg=/DRIVER");
                println!("cargo:rustc-cdylib-link-arg=/NODEFAULTLIB");
                println!("cargo:rustc-cdylib-link-arg=/SUBSYSTEM:NATIVE");
                println!("cargo:rustc-cdylib-link-arg=/KERNEL");

                // Linker arguments derived from WindowsDriver.KernelMode.KMDFConfig.props in
                // Ni(22H2) WDK
                println!("cargo:rustc-cdylib-link-arg=/ENTRY:FxDriverEntry");
            }
            DriverConfig::UMDFConfig(_) => {
                // Linker arguments derived from WindowsDriver.UserMode.props in Ni(22H2) WDK
                println!("cargo:rustc-cdylib-link-arg=/SUBSYSTEM:WINDOWS");
            }
        }
    }

    /// Serializes this [`Config`] and exports it via the Cargo
    /// `DEP_<CARGO_MANIFEST_LINKS>_WDK_CONFIG` environment variable.
    ///
    /// # Errors
    ///
    /// This function will return an error if the crate does not have a `links`
    /// field in its Cargo manifest or if it fails to serialize the config.
    ///
    /// # Panics
    ///
    /// Panics if this [`Config`] fails to serialize.
    pub fn export_config(&self) -> Result<(), ExportError> {
        if let Err(var_error) = std::env::var("CARGO_MANIFEST_LINKS") {
            return Err(ExportError::MissingLinksValue(var_error));
        }
        println!(
            "cargo:{}={}",
            Self::CARGO_CONFIG_KEY,
            serde_json::to_string(self)?
        );
        Ok(())
    }
}

impl Default for KMDFConfig {
    #[must_use]
    fn default() -> Self {
        // FIXME: determine default values from TargetVersion and _NT_TARGET_VERSION
        Self {
            kmdf_version_major: 1,
            kmdf_version_minor: 33,
        }
    }
}

impl KMDFConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for UMDFConfig {
    #[must_use]
    fn default() -> Self {
        // FIXME: determine default values from TargetVersion and _NT_TARGET_VERSION
        Self {
            umdf_version_major: 2,
            umdf_version_minor: 33,
        }
    }
}

impl UMDFConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CPUArchitecture {
    #[must_use]
    pub const fn to_windows_str(&self) -> &str {
        match self {
            Self::AMD64 => "x64",
            Self::ARM64 => "ARM64",
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(nightly_toolchain)]
    use std::assert_matches::assert_matches;
    use std::{collections::HashMap, ffi::OsStr, sync::Mutex};

    use super::*;

    /// Runs function after modifying environment variables, and returns the
    /// function's return value.
    ///
    /// The environment is guaranteed to be not modified during the execution
    /// of the function, and the environment is reset to its original state
    /// after execution of the function. No testing asserts should be called in
    /// the function, since a failing test will poison the mutex, and cause all
    /// remaining tests to fail.
    ///
    /// # Panics
    ///
    /// Panics if called with duplicate environment variable keys.
    fn with_env<K, V, F, R>(env_vars_key_value_pairs: &[(K, V)], f: F) -> R
    where
        K: AsRef<OsStr> + std::cmp::Eq + std::hash::Hash,
        V: AsRef<OsStr>,
        F: FnOnce() -> R,
    {
        // Tests can execute in multiple threads in the same process, so mutex must be
        // used to guard access to the environment variables
        static ENV_MUTEX: Mutex<()> = Mutex::new(());

        let _mutex_guard = ENV_MUTEX.lock().unwrap();
        let mut original_env_vars = HashMap::new();

        // set requested environment variables
        for (key, value) in env_vars_key_value_pairs {
            if let Ok(original_value) = std::env::var(key) {
                let insert_result = original_env_vars.insert(key, original_value);
                assert!(
                    insert_result.is_none(),
                    "Duplicate environment variable keys were provided"
                );
            }
            std::env::set_var(key, value);
        }

        let f_return_value = f();

        // reset all set environment variables
        for (key, _) in env_vars_key_value_pairs {
            original_env_vars.get(key).map_or_else(
                || {
                    std::env::remove_var(key);
                },
                |value| {
                    std::env::set_var(key, value);
                },
            );
        }

        f_return_value
    }

    #[test]
    fn default_options() {
        let wdk_build_options = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], Config::new);

        #[cfg(nightly_toolchain)]
        assert_matches!(wdk_build_options.driver_config, DriverConfig::WDM());
        assert_eq!(wdk_build_options.cpu_architecture, CPUArchitecture::AMD64);
    }

    #[test]
    fn wdm_options() {
        let wdk_build_options = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
            driver_config: DriverConfig::WDM(),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(wdk_build_options.driver_config, DriverConfig::WDM());
        assert_eq!(wdk_build_options.cpu_architecture, CPUArchitecture::AMD64);
    }

    #[test]
    fn default_kmdf_options() {
        let wdk_build_options = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
            driver_config: DriverConfig::KMDFConfig(KMDFConfig::new()),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(
            wdk_build_options.driver_config,
            DriverConfig::KMDFConfig(KMDFConfig {
                kmdf_version_major: 1,
                kmdf_version_minor: 33
            })
        );
        assert_eq!(wdk_build_options.cpu_architecture, CPUArchitecture::AMD64);
    }

    #[test]
    fn kmdf_options() {
        let wdk_build_options = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
            driver_config: DriverConfig::KMDFConfig(KMDFConfig {
                kmdf_version_major: 1,
                kmdf_version_minor: 15,
            }),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(
            wdk_build_options.driver_config,
            DriverConfig::KMDFConfig(KMDFConfig {
                kmdf_version_major: 1,
                kmdf_version_minor: 15
            })
        );
        assert_eq!(wdk_build_options.cpu_architecture, CPUArchitecture::AMD64);
    }

    #[test]
    fn default_umdf_options() {
        let wdk_build_options = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
            driver_config: DriverConfig::UMDFConfig(UMDFConfig::new()),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(
            wdk_build_options.driver_config,
            DriverConfig::UMDFConfig(UMDFConfig {
                umdf_version_major: 2,
                umdf_version_minor: 33
            })
        );
        assert_eq!(wdk_build_options.cpu_architecture, CPUArchitecture::AMD64);
    }

    #[test]
    fn umdf_options() {
        let wdk_build_options = with_env(&[("CARGO_CFG_TARGET_ARCH", "aarch64")], || Config {
            driver_config: DriverConfig::UMDFConfig(UMDFConfig {
                umdf_version_major: 2,
                umdf_version_minor: 15,
            }),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(
            wdk_build_options.driver_config,
            DriverConfig::UMDFConfig(UMDFConfig {
                umdf_version_major: 2,
                umdf_version_minor: 15
            })
        );
        assert_eq!(wdk_build_options.cpu_architecture, CPUArchitecture::ARM64);
    }
}
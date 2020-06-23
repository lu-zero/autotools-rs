//! A build dependency for running the correct autotools commands to build a native library
//!
//! This crate provides the facilities to setup the build system and build native libraries
//! that leverage `autotools` or `configure & make` workalike scripts.
//!
//! ## Installation
//!
//! Add to your `Cargo.toml` a build dependency:
//!
//! ```toml
//! [build-dependencies]
//! autotools = "0.3"
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use autotools;
//!
//! // Build the project in the path `foo` and installs it in `$OUT_DIR`
//! let dst = autotools::build("foo");
//!
//! // Simply link the library without using pkg-config
//! println!("cargo:rustc-link-search=native={}", dst.display());
//! println!("cargo:rustc-link-lib=static=foo");
//! ```
//!
//! ```no_run
//! use autotools::Config;
//!
//! let dst = Config::new("foo")
//!     .reconf("-ivf")
//!     .enable("feature", None)
//!     .with("dep", None)
//!     .disable("otherfeature", None)
//!     .without("otherdep", None)
//!     .cflag("-Wall")
//!     .build();
//! ```

extern crate cc;

use std::env;
use std::ffi::{OsString, OsStr};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

enum Kind {
    Enable,
    Disable,
    With,
    Without,
    Arbitrary,
}

/// Builder style configuration for a pending autotools build.
///
/// # Note
///
/// Note that `host` and `target` have different meanings for Rust
/// than for Gnu autotools. For Rust, the "host" machine is the one where the
/// compiler is running, and the "target" machine is the one where the
/// compiled artifact (library or binary) will be executed.
/// For Gnu autotools, the machine where the compiler is running is called
/// the "build" machine; the one where the compiled artifact will be
/// executed is called the "host" machine; and if the compiled artifact
/// happens to be a cross-compiler, it will generate code for a "target"
/// machine; otherwise "target" will coincide with "host".
///
/// Hence Rust's `host` corresponds to Gnu autotools' "build" and Rust's
/// `target` corresponds to their "host" (though the relevant names will sometimes
/// differ slightly).
///
/// The `host` and `target` methods on this package's `autotools::Config` structure (as well as
/// the `$HOST` and `$TARGET` variables set by cargo) are understood with their
/// Rust meaning.
///
/// When cross-compiling, we try to calculate automatically what Gnu autotools will expect for its
/// "host" value, and supply that to the `configure` script using a `--host="..."` argument. If the
/// auto-calculation is incorrect, you can override it with the `config_option` method, like this:
///
/// ```no_run
/// use autotools;
///
/// // Builds the project in the directory located in `libfoo`, installing it
/// // into $OUT_DIR
/// let mut cfg = autotools::Config::new("libfoo_source_directory");
/// cfg.config_option("host", Some("i686-pc-windows-gnu"));
/// let dst = cfg.build();
///
/// println!("cargo:rustc-link-search=native={}", dst.display());
/// println!("cargo:rustc-link-lib=static=foo");
/// ```
pub struct Config {
    enable_shared: bool,
    enable_static: bool,
    path: PathBuf,
    cflags: OsString,
    cxxflags: OsString,
    ldflags: OsString,
    options: Vec<(Kind, OsString, Option<OsString>)>,
    target: Option<String>,
    make_args: Option<Vec<String>>,
    make_targets: Option<Vec<String>>,
    host: Option<String>,
    out_dir: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    reconfig: Option<OsString>,
    build_insource: bool,
}

/// Builds the native library rooted at `path` with the default configure options.
/// This will return the directory in which the library was installed.
///
/// # Examples
///
/// ```no_run
/// use autotools;
///
/// // Builds the project in the directory located in `libfoo`, installing it
/// // into $OUT_DIR
/// let dst = autotools::build("libfoo");
///
/// println!("cargo:rustc-link-search=native={}", dst.display());
/// println!("cargo:rustc-link-lib=static=foo");
/// ```
///
pub fn build<P: AsRef<Path>>(path: P) -> PathBuf {
    Config::new(path.as_ref())
        .build()

}

impl Config {
    /// Creates a new blank set of configuration to build the project specified
    /// at the path `path`.
    pub fn new<P: AsRef<Path>>(path: P) -> Config {
        Config {
            enable_shared: false,
            enable_static: true,
            path: env::current_dir().unwrap().join(path),
            cflags: OsString::new(),
            cxxflags: OsString::new(),
            ldflags: OsString::new(),
            options: Vec::new(),
            make_args: None,
            make_targets: None,
            out_dir: None,
            target: None,
            host: None,
            env: Vec::new(),
            reconfig: None,
            build_insource: false,
        }
    }

    /// Enables building as a shared library (`--enable-shared`).
    pub fn enable_shared(&mut self) -> &mut Config {
        self.enable_shared = true;
        self
    }

    /// Disables building as a shared library (`--disable-shared`).
    pub fn disable_shared(&mut self) -> &mut Config {
        self.enable_shared = false;
        self
    }

    /// Enables building as a static library (`--enable-static`).
    pub fn enable_static(&mut self) -> &mut Config {
        self.enable_static = true;
        self
    }

    /// Disables building as a static library (`--disable-static`).
    pub fn disable_static(&mut self) -> &mut Config {
        self.enable_static = false;
        self
    }

    /// Additional arguments to pass through to `make`.
    pub fn make_args(&mut self, flags: Vec<String>) -> &mut Config {
        self.make_args = Some(flags);
        self
    }

    fn set_opt<P: AsRef<OsStr>>(&mut self, kind: Kind, opt: P, optarg: Option<P>) -> &mut Config {
        let optarg = optarg.as_ref().map(|v| v.as_ref().to_owned());
        self.options.push((kind, opt.as_ref().to_owned(),
                           optarg));
        self
    }

    /// Passes `--<opt><=optarg>` to configure.
    pub fn config_option<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::Arbitrary, opt, optarg)
    }

    /// Passes `--enable-<opt><=optarg>` to configure.
    pub fn enable<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::Enable, opt, optarg)
    }

    /// Passes `--disable-<opt><=optarg>` to configure.
    pub fn disable<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::Disable, opt, optarg)
    }

    /// Passes `--with-<opt><=optarg>` to configure.
    pub fn with<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::With, opt, optarg)
    }

    /// Passes `--without-<opt><=optarg>` to configure.
    pub fn without<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::Without, opt, optarg)
    }

    /// Adds a custom flag to pass down to the C compiler, supplementing those
    /// that this library already passes.
    ///
    /// Default flags for the chosen compiler have lowest priority, then any
    /// flags from the environment variable `$CFLAGS`, then any flags specified
    /// with this method.
    pub fn cflag<P: AsRef<OsStr>>(&mut self, flag: P) -> &mut Config {
        self.cflags.push(" ");
        self.cflags.push(flag.as_ref());
        self
    }

    /// Adds a custom flag to pass down to the C++ compiler, supplementing those
    /// that this library already passes.
    ///
    /// Default flags for the chosen compiler have lowest priority, then any
    /// flags from the environment variable `$CXXFLAGS`, then any flags specified
    /// with this method.
    pub fn cxxflag<P: AsRef<OsStr>>(&mut self, flag: P) -> &mut Config {
        self.cxxflags.push(" ");
        self.cxxflags.push(flag.as_ref());
        self
    }

    /// Adds a custom flag to pass down to the linker, supplementing those
    /// that this library already passes.
    ///
    /// Flags from the environment variable `$LDFLAGS` have lowest priority,
    /// then any flags specified with this method.
    pub fn ldflag<P: AsRef<OsStr>>(&mut self, flag: P) -> &mut Config {
        self.ldflags.push(" ");
        self.ldflags.push(flag.as_ref());
        self
    }

    /// Sets the target triple for this compilation.
    ///
    /// This is automatically scraped from `$TARGET` which is set for Cargo
    /// build scripts so it's not necessary to call this from a build script.
    ///
    /// See [Note](#main) on the differences between Rust's and autotools'
    /// interpretation of "target" (this method assumes the former).
    pub fn target(&mut self, target: &str) -> &mut Config {
        self.target = Some(target.to_string());
        self
    }

    /// Sets the host triple for this compilation.
    ///
    /// This is automatically scraped from `$HOST` which is set for Cargo
    /// build scripts so it's not necessary to call this from a build script.
    ///
    /// See [Note](#main) on the differences between Rust's and autotools'
    /// interpretation of "host" (this method assumes the former).
    pub fn host(&mut self, host: &str) -> &mut Config {
        self.host = Some(host.to_string());
        self
    }

    /// Sets the output directory for this compilation.
    ///
    /// This is automatically scraped from `$OUT_DIR` which is set for Cargo
    /// build scripts so it's not necessary to call this from a build script.
    pub fn out_dir<P: AsRef<Path>>(&mut self, out: P) -> &mut Config {
        self.out_dir = Some(out.as_ref().to_path_buf());
        self
    }

    /// Configure an environment variable for the `configure && make` processes
    /// spawned by this crate in the `build` step.
    ///
    /// If you want to set `$CFLAGS`, `$CXXFLAGS`, or `$LDFLAGS`, consider using the
    /// [cflag](#method.cflag),
    /// [cxxflag](#method.cxxflag), or
    /// [ldflag](#method.ldflag)
    /// methods instead, which will append to any external
    /// values. Setting those environment variables here will overwrite the
    /// external values, and will also discard any flags determined by the chosen
    /// compiler.
    ///
    /// `autotools::Config` will automatically pass `$CC` and `$CXX` values to
    /// the `configure` script based on the chosen compiler. Setting those
    /// variables here will override, and interferes with other parts of this
    /// library, so is not recommended.
    pub fn env<K, V>(&mut self, key: K, value: V) -> &mut Config
        where K: AsRef<OsStr>,
              V: AsRef<OsStr>,
    {
        self.env.push((key.as_ref().to_owned(), value.as_ref().to_owned()));
        self
    }

    /// Options to pass through to `autoreconf` prior to configuring the build.
    pub fn reconf<P: AsRef<OsStr>>(&mut self, flags: P) -> &mut Config {
        self.reconfig = Some(flags.as_ref().to_os_string());
        self
    }

    /// Build the given make target.
    ///
    /// If this function is not called, the build will default to `make install`.
    pub fn make_target(&mut self, make_target: &str) -> &mut Config {
        self.make_targets.get_or_insert_with(Vec::new).push(make_target.to_owned());
        self
    }

    /// Build the library in-source.
    ///
    /// This is generally not recommended, but can be required for libraries that
    /// make extensive use of nested Makefiles, which cannot be included in
    /// out-of-source builds.
    pub fn insource(&mut self, build_insource: bool) -> &mut Config {
        self.build_insource = build_insource;
        self
    }

    /// Run this configuration, compiling the library with all the configured
    /// options.
    ///
    /// This will run both the build system generator command as well as the
    /// command to build the library.
    pub fn build(&mut self) -> PathBuf {
        let target = self.target.clone().unwrap_or_else(|| {
                getenv_unwrap("TARGET")
        });
        let host = self.host.clone().unwrap_or_else(|| {
            getenv_unwrap("HOST")
        });
        let mut c_cfg = cc::Build::new();
        c_cfg.cargo_metadata(false)
            .target(&target)
            .warnings(false)
            .host(&host);
        let mut cxx_cfg = cc::Build::new();
        cxx_cfg.cargo_metadata(false)
            .cpp(true)
            .target(&target)
            .warnings(false)
            .host(&host);
        let c_compiler = c_cfg.get_compiler();
        let cxx_compiler = cxx_cfg.get_compiler();

        let dst;
        let build;

        if self.build_insource {
            dst = self.path.clone();
            build = dst.clone();
        } else {
            dst = self.out_dir.clone().unwrap_or_else(|| {
                PathBuf::from(getenv_unwrap("OUT_DIR"))
            });
            build = dst.join("build");
            self.maybe_clear(&build);
            let _ = fs::create_dir(&build);
        }

        // TODO: env overrides?
        // TODO: PKG_CONFIG_PATH
        if let Some(ref opts) = self.reconfig {
            let executable = "autoreconf".to_owned();
            let mut cmd = Command::new(executable);
            cmd.current_dir(&self.path);

            run(cmd.arg(opts), "autoreconf");
        }

        let executable = PathBuf::from(&self.path).join("configure");
        let mut cmd = Command::new(executable);

        cmd.arg(format!("--prefix={}", dst.display()));
        if self.enable_shared {
            cmd.arg("--enable-shared");
        } else {
            cmd.arg("--disable-shared");
        }

        if self.enable_static {
            cmd.arg("--enable-static");
        } else {
            cmd.arg("--disable-static");
        }

        let mut cflags = c_compiler.cflags_env();
        match env::var_os("CFLAGS") {
            None => (),
            Some(flags) => {
                cflags.push(" ");
                cflags.push(&flags);
            }
        }
        if !self.cflags.is_empty() {
            cflags.push(" ");
            cflags.push(&self.cflags);
        }
        cmd.env("CFLAGS", cflags);

        let mut cxxflags = cxx_compiler.cflags_env();
        match env::var_os("CXXFLAGS") {
            None => (),
            Some(flags) => {
                cxxflags.push(" ");
                cxxflags.push(&flags);
            }
        }
        if !self.cxxflags.is_empty() {
            cxxflags.push(" ");
            cxxflags.push(&self.cxxflags);
        }
        cmd.env("CXXFLAGS", cxxflags);

        if !self.ldflags.is_empty() {
            match env::var_os("LDFLAGS") {
                None => cmd.env("LDFLAGS", &self.ldflags),
                Some(flags) => {
                    let mut os = OsString::from(flags);
                    os.push(" ");
                    os.push(&self.ldflags);
                    cmd.env("LDFLAGS", &os)
                }
            };
        }

        let mut config_host = false;

        for &(ref kind, ref k, ref v) in &self.options {
            let mut os = OsString::from("--");
            match *kind {
                Kind::Enable => os.push("enable-"),
                Kind::Disable => os.push("disable-"),
                Kind::With => os.push("with-"),
                Kind::Without => os.push("without-"),
                Kind::Arbitrary => {
                    if k == "host" {
                        config_host = true;
                    }
                }
            };
            os.push(k);
            if let &Some(ref v) = v {
                os.push("=");
                os.push(v);
            }
            cmd.arg(os);
        }

        if !config_host {
            let compiler_path = format!("--host={}", c_compiler.path().display());
            if compiler_path != "--host=musl-gcc" && compiler_path.ends_with("-gcc") {
                cmd.arg(&compiler_path[0..compiler_path.len() - 4]);
            }
        }

        cmd.env("CC", c_compiler.path().to_str().unwrap());
        cmd.env("CXX", cxx_compiler.path().to_str().unwrap());

        for &(ref k, ref v) in c_compiler.env().iter().chain(&self.env) {
            cmd.env(k, v);
        }

        for &(ref k, ref v) in cxx_compiler.env().iter().chain(&self.env) {
            cmd.env(k, v);
        }

        run_config(cmd.current_dir(&build), &build);

        // Build up the first make command to build the build system.
        let executable = env::var("MAKE").unwrap_or("make".to_owned());
        let mut cmd = Command::new(executable);
        cmd.current_dir(&build);

        let mut makeflags = None;
        let mut make_args = Vec::new();

        if let Some(args) = &self.make_args {
            make_args.extend_from_slice(args);
        }

        if let Ok(s) = env::var("NUM_JOBS") {
            match env::var_os("CARGO_MAKEFLAGS") {
                // Only do this on non-windows and non-bsd
                // On Windows, we could be invoking make instead of
                // mingw32-make which doesn't work with our jobserver
                // bsdmake also does not work with our job server
                Some(ref s) if !(cfg!(windows) ||
                                 cfg!(target_os = "openbsd") ||
                                 cfg!(target_os = "netbsd") ||
                                 cfg!(target_os = "freebsd") ||
                                 cfg!(target_os = "bitrig") ||
                                 cfg!(target_os = "dragonflybsd")
                ) => makeflags = Some(s.clone()),

                // This looks like `make`, let's hope it understands `-jN`.
                _ => make_args.push(format!("-j{}", s)),
            }
        }

        // And build!
        let make_targets = self.make_targets
            .get_or_insert(vec!["install".to_string()]);
        if let Some(flags) = makeflags {
            cmd.env("MAKEFLAGS", flags);
        }

        run(cmd.args(make_targets)
                .args(&make_args)
                .current_dir(&build), "make");

        println!("cargo:root={}", dst.display());
        dst
    }

    fn maybe_clear(&self, _dir: &Path) {
        // TODO: make clean?
    }
}

fn run_config(cmd: &mut Command, path: &Path) {
    println!("running: {:?}", cmd);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(ref e) if e.kind() == ErrorKind::NotFound => {
            fail(&format!("failed to execute command: {}\nis `configure` not installed?",
                          e));
        }
        Err(e) => fail(&format!("failed to execute command: {}", e)),
    };
    if !status.success() {
        let executable = "cat".to_owned();
        let mut cmd = Command::new(executable);
        cmd.current_dir(path);
        return run(cmd.arg("config.log"), "cat config.log");
    }
}

fn run(cmd: &mut Command, program: &str) {
    println!("running: {:?}", cmd);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(ref e) if e.kind() == ErrorKind::NotFound => {
            fail(&format!("failed to execute command: {}\nis `{}` not installed?",
                          e, program));
        }
        Err(e) => fail(&format!("failed to execute command: {}", e)),
    };
    if !status.success() {
        fail(&format!("command did not execute successfully, got: {}", status));
    }
}

fn getenv_unwrap(v: &str) -> String {
    match env::var(v) {
        Ok(s) => s,
        Err(..) => fail(&format!("environment variable `{}` not defined", v)),
    }
}

fn fail(s: &str) -> ! {
    panic!("\n{}\n\nbuild script failed, must exit now", s)
}

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
//! autotools = "0.1"
//!
//! ## Usage
//!
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
//!     .enable("feature")
//!     .with("dep")
//!     .disable("otherfeature")
//!     .without("otherdep")
//!     .cflag("-Wall")
//!     .build()
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
}

/// Builder style configuration for a pending autotools build.
pub struct Config {
    path: PathBuf,
    cflags: OsString,
    cxxflags: OsString,
    options: Vec<(Kind, OsString, Option<OsString>)>,
    target: Option<String>,
    make_args: Option<Vec<String>>,
    host: Option<String>,
    out_dir: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    reconfig: Option<OsString>,
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
            path: env::current_dir().unwrap().join(path),
            cflags: OsString::new(),
            cxxflags: OsString::new(),
            options: Vec::new(),
            make_args: None,
            out_dir: None,
            target: None,
            host: None,
            env: Vec::new(),
            reconfig: None,
        }
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

    /// --enable-<opt><=optarg>
    pub fn enable<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::Enable, opt, optarg)
    }

    /// --disable-<opt><=optarg>
    pub fn disable<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::Disable, opt, optarg)
    }

    /// --with-<opt><=optarg>
    pub fn with<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::With, opt, optarg)
    }

    /// --without-<opt><=optarg>
    pub fn without<P: AsRef<OsStr>>(&mut self, opt: P, optarg: Option<P>) -> &mut Config {
        self.set_opt(Kind::Without, opt, optarg)
    }

    /// Adds a custom flag to pass down to the C compiler, supplementing those
    /// that this library already passes.
    pub fn cflag<P: AsRef<OsStr>>(&mut self, flag: P) -> &mut Config {
        self.cflags.push(" ");
        self.cflags.push(flag.as_ref());
        self
    }

    /// Adds a custom flag to pass down to the C++ compiler, supplementing those
    /// that this library already passes.
    pub fn cxxflag<P: AsRef<OsStr>>(&mut self, flag: P) -> &mut Config {
        self.cxxflags.push(" ");
        self.cxxflags.push(flag.as_ref());
        self
    }

    /// Sets the target triple for this compilation.
    ///
    /// This is automatically scraped from `$TARGET` which is set for Cargo
    /// build scripts so it's not necessary to call this from a build script.
    pub fn target(&mut self, target: &str) -> &mut Config {
        self.target = Some(target.to_string());
        self
    }

    /// Sets the host triple for this compilation.
    ///
    /// This is automatically scraped from `$HOST` which is set for Cargo
    /// build scripts so it's not necessary to call this from a build script.
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
            .opt_level(0)
            .debug(false)
            .target(&target)
            .warnings(false)
            .host(&host);
        let mut cxx_cfg = cc::Build::new();
        cxx_cfg.cargo_metadata(false)
            .cpp(true)
            .opt_level(0)
            .debug(false)
            .target(&target)
            .warnings(false)
            .host(&host);
        let c_compiler = c_cfg.get_compiler();
        let cxx_compiler = cxx_cfg.get_compiler();

        let dst = self.out_dir.clone().unwrap_or_else(|| {
            PathBuf::from(getenv_unwrap("OUT_DIR"))
        });
        let build = dst.join("build");
        self.maybe_clear(&build);
        let _ = fs::create_dir(&build);

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
        // TODO: make it optional?
        cmd.arg("--disable-shared").arg("--enable-static");

        for &(ref kind, ref k, ref v) in &self.options {
            let mut os = OsString::from("--");
            match *kind {
                Kind::Enable => os.push("enable"),
                Kind::Disable => os.push("disable"),
                Kind::With => os.push("with"),
                Kind::Without => os.push("without")
            };
            os.push(k);
            if let &Some(ref v) = v {
                os.push("=");
                os.push(v);
            }
            cmd.arg(os);
        }

        for &(ref k, ref v) in c_compiler.env().iter().chain(&self.env) {
            cmd.env(k, v);
        }

        for &(ref k, ref v) in cxx_compiler.env().iter().chain(&self.env) {
            cmd.env(k, v);
        }

        run(cmd.current_dir(&build), "configure");

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
        let target = // self.cmake_target.clone().unwrap_or(
            "install".to_string();
        if let Some(flags) = makeflags {
            cmd.env("MAKEFLAGS", flags);
        }

        run(cmd.arg(target)
                .args(&make_args)
                .current_dir(&build), "make");

        println!("cargo:root={}", dst.display());
        dst
    }

    fn maybe_clear(&self, _dir: &Path) {
        // TODO: make clean?
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

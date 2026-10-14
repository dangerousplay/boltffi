use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::config::DebugSymbolsConfig;
use crate::config::targets::kotlin::default_android_min_sdk;
use crate::target::JavaJvmHostTarget;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct JavaConfig {
    pub package: Option<String>,
    pub module_name: Option<String>,
    pub min_version: Option<u8>,
    #[serde(default)]
    pub jvm: JavaJvmConfig,
    #[serde(default)]
    pub android: JavaAndroidConfig,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JavaJvmConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_java_jvm_output")]
    pub output: PathBuf,
    pub host_targets: Option<Vec<JavaJvmHostTarget>>,
    #[serde(default)]
    pub strip_symbols: bool,
    #[serde(default)]
    pub debug_symbols: DebugSymbolsConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jni_compiler: Option<JniCompilerCommand>,
}

/// `[targets.java.jvm].jni_compiler`: the program that compiles the JNI glue
/// and links the JNI library, plus its leading arguments (`"zig cc"`,
/// `"clang -fuse-ld=lld"`). Split on whitespace like `[cargo].build_command`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct JniCompilerCommand {
    program: String,
    args: Vec<String>,
}

impl JniCompilerCommand {
    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }
}

impl FromStr for JniCompilerCommand {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut tokens = value.split_whitespace().map(str::to_string);
        match tokens.next() {
            None => Err("targets.java.jvm.jni_compiler must not be empty".to_string()),
            Some(program) if program.starts_with('-') => Err(format!(
                "targets.java.jvm.jni_compiler must start with a program name, got '{program}'"
            )),
            Some(program) => Ok(Self {
                program,
                args: tokens.collect(),
            }),
        }
    }
}

impl TryFrom<String> for JniCompilerCommand {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<JniCompilerCommand> for String {
    fn from(command: JniCompilerCommand) -> Self {
        command.to_string()
    }
}

impl fmt::Display for JniCompilerCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.program)?;
        self.args
            .iter()
            .try_for_each(|arg| write!(formatter, " {arg}"))
    }
}

impl Default for JavaJvmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: default_java_jvm_output(),
            host_targets: None,
            strip_symbols: false,
            debug_symbols: DebugSymbolsConfig::default(),
            jni_compiler: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JavaAndroidConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_java_android_output")]
    pub output: PathBuf,
    #[serde(default = "default_android_min_sdk")]
    pub min_sdk: u32,
}

impl Default for JavaAndroidConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: default_java_android_output(),
            min_sdk: default_android_min_sdk(),
        }
    }
}

fn default_java_jvm_output() -> PathBuf {
    PathBuf::from("dist/java")
}

fn default_java_android_output() -> PathBuf {
    PathBuf::from("dist/java/android")
}

#[cfg(test)]
mod tests {
    use super::JniCompilerCommand;

    #[test]
    fn jni_compiler_splits_program_and_leading_arguments() {
        let single = "clang".parse::<JniCompilerCommand>().unwrap();
        assert_eq!((single.program(), single.args()), ("clang", &[][..]));

        let zig = "  zig   cc ".parse::<JniCompilerCommand>().unwrap();
        assert_eq!(zig.program(), "zig");
        assert_eq!(zig.args(), ["cc"]);
        assert_eq!(zig.to_string(), "zig cc");

        let clang = "clang -fuse-ld=lld".parse::<JniCompilerCommand>().unwrap();
        assert_eq!(clang.program(), "clang");
        assert_eq!(clang.args(), ["-fuse-ld=lld"]);
    }

    #[test]
    fn jni_compiler_rejects_empty_values_and_leading_flags() {
        for value in ["", "   "] {
            assert_eq!(
                value.parse::<JniCompilerCommand>(),
                Err("targets.java.jvm.jni_compiler must not be empty".to_string())
            );
        }
        assert!(
            "-target x86_64-linux-gnu"
                .parse::<JniCompilerCommand>()
                .unwrap_err()
                .contains("must start with a program name, got '-target'")
        );
    }
}

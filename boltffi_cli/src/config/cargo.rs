use std::collections::HashMap;
use std::fmt;
use std::process::Command;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CargoConfig {
    #[serde(default)]
    pub global_args: Vec<String>,
    #[serde(default)]
    pub command_args: HashMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "CargoBuildCommand::is_default")]
    pub build_command: CargoBuildCommand,
}

/// The Cargo operation a library build runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoOperation {
    /// `cargo build`: plain library builds.
    Build,
    /// `cargo rustc`: builds that pass rustc flags (binding expansion, link probes, metadata).
    Rustc,
}

/// The program that runs library builds, set by `[cargo].build_command` or `--cargo-build-cmd`.
///
/// | Value | Build | Rustc |
/// |-------|-------|-------|
/// | `cargo` (default) | `cargo build` | `cargo rustc` |
/// | `zigbuild` | `cargo-zigbuild zigbuild` | `cargo-zigbuild rustc` |
/// | `cross` | `cross build` | `cross rustc` |
/// | `"<program>"` | `<program> build` | `<program> rustc` |
/// | `"<program> <subcommand…>"` | `<program> <subcommand…>` | `<program> <subcommand…>` |
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum CargoBuildCommand {
    #[default]
    Cargo,
    Zigbuild,
    Cross,
    Custom {
        program: String,
        subcommand: Vec<String>,
    },
}

impl CargoBuildCommand {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Cargo)
    }

    pub fn program(&self) -> &str {
        match self {
            Self::Cargo => "cargo",
            Self::Zigbuild => "cargo-zigbuild",
            Self::Cross => "cross",
            Self::Custom { program, .. } => program,
        }
    }

    pub fn subcommand(&self, operation: CargoOperation) -> Vec<&str> {
        match (self, operation) {
            (Self::Custom { subcommand, .. }, _) if !subcommand.is_empty() => {
                subcommand.iter().map(String::as_str).collect()
            }
            (Self::Zigbuild, CargoOperation::Build) => vec!["zigbuild"],
            (_, CargoOperation::Build) => vec!["build"],
            (_, CargoOperation::Rustc) => vec!["rustc"],
        }
    }

    /// Starts a command for `operation`. A `+toolchain` selector is passed as an
    /// argument only to `cargo`; other programs receive it as `RUSTUP_TOOLCHAIN`.
    pub fn command(&self, operation: CargoOperation, toolchain_selector: Option<&str>) -> Command {
        let mut command = Command::new(self.program());
        if let Some(toolchain_selector) = toolchain_selector {
            if self.program() == "cargo" {
                command.arg(toolchain_selector);
            } else {
                command.env(
                    "RUSTUP_TOOLCHAIN",
                    toolchain_selector.trim_start_matches('+'),
                );
            }
        }
        command.args(self.subcommand(operation));
        command
    }
}

impl FromStr for CargoBuildCommand {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let tokens = value.split_whitespace().collect::<Vec<_>>();
        match tokens.as_slice() {
            [] => Err("cargo build command must not be empty".to_string()),
            [program, ..] if program.starts_with(['-', '+']) => Err(format!(
                "cargo build command must start with a program name, got '{program}'"
            )),
            ["cargo"] | ["cargo", "build"] => Ok(Self::Cargo),
            ["zigbuild"]
            | ["cargo-zigbuild"]
            | ["cargo", "zigbuild"]
            | ["cargo-zigbuild", "zigbuild"] => Ok(Self::Zigbuild),
            ["cross"] | ["cross", "build"] => Ok(Self::Cross),
            [program, subcommand @ ..] => Ok(Self::Custom {
                program: program.to_string(),
                subcommand: subcommand.iter().map(|token| token.to_string()).collect(),
            }),
        }
    }
}

impl TryFrom<String> for CargoBuildCommand {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<CargoBuildCommand> for String {
    fn from(command: CargoBuildCommand) -> Self {
        command.to_string()
    }
}

impl fmt::Display for CargoBuildCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cargo => f.write_str("cargo"),
            Self::Zigbuild => f.write_str("zigbuild"),
            Self::Cross => f.write_str("cross"),
            Self::Custom {
                program,
                subcommand,
            } => {
                f.write_str(program)?;
                subcommand
                    .iter()
                    .try_for_each(|token| write!(f, " {token}"))
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageConfig {
    pub name: String,
    #[serde(rename = "crate")]
    pub crate_name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{CargoBuildCommand, CargoConfig, CargoOperation};

    fn parse(value: &str) -> CargoBuildCommand {
        value.parse().expect("build command should parse")
    }

    fn invocation(
        command: &CargoBuildCommand,
        operation: CargoOperation,
        toolchain_selector: Option<&str>,
    ) -> (String, Vec<String>, Option<String>) {
        let command = command.command(operation, toolchain_selector);
        let toolchain_env = command
            .get_envs()
            .find(|(key, _)| *key == "RUSTUP_TOOLCHAIN")
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned());
        (
            command.get_program().to_string_lossy().into_owned(),
            command
                .get_args()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect(),
            toolchain_env,
        )
    }

    #[test]
    fn parses_known_build_commands() {
        assert_eq!(parse("cargo"), CargoBuildCommand::Cargo);
        assert_eq!(parse(" cargo  build "), CargoBuildCommand::Cargo);
        assert_eq!(parse("zigbuild"), CargoBuildCommand::Zigbuild);
        assert_eq!(parse("cargo zigbuild"), CargoBuildCommand::Zigbuild);
        assert_eq!(parse("cargo-zigbuild"), CargoBuildCommand::Zigbuild);
        assert_eq!(parse("cross"), CargoBuildCommand::Cross);
        assert_eq!(parse("cross build"), CargoBuildCommand::Cross);
    }

    #[test]
    fn parses_custom_build_commands_with_and_without_subcommand() {
        assert_eq!(
            parse("my-cross"),
            CargoBuildCommand::Custom {
                program: "my-cross".to_string(),
                subcommand: Vec::new(),
            }
        );
        assert_eq!(
            parse("cargo xwin rustc"),
            CargoBuildCommand::Custom {
                program: "cargo".to_string(),
                subcommand: vec!["xwin".to_string(), "rustc".to_string()],
            }
        );
        assert_eq!(parse("cargo xwin rustc").to_string(), "cargo xwin rustc");
    }

    #[test]
    fn rejects_empty_and_flag_build_commands() {
        assert!("".parse::<CargoBuildCommand>().is_err());
        assert!("   ".parse::<CargoBuildCommand>().is_err());
        assert!("--release".parse::<CargoBuildCommand>().is_err());
        assert!("+nightly build".parse::<CargoBuildCommand>().is_err());
    }

    #[test]
    fn reads_build_command_from_toml() {
        let config: CargoConfig = toml::from_str("build_command = \"zigbuild\"").unwrap();
        assert_eq!(config.build_command, CargoBuildCommand::Zigbuild);

        let config: CargoConfig = toml::from_str("").unwrap();
        assert_eq!(config.build_command, CargoBuildCommand::Cargo);

        assert!(toml::from_str::<CargoConfig>("build_command = \"\"").is_err());
    }

    #[test]
    fn assembles_build_and_rustc_invocations() {
        let cases = [
            ("cargo", CargoOperation::Build, "cargo", vec!["build"]),
            ("cargo", CargoOperation::Rustc, "cargo", vec!["rustc"]),
            (
                "zigbuild",
                CargoOperation::Build,
                "cargo-zigbuild",
                vec!["zigbuild"],
            ),
            (
                "zigbuild",
                CargoOperation::Rustc,
                "cargo-zigbuild",
                vec!["rustc"],
            ),
            ("cross", CargoOperation::Build, "cross", vec!["build"]),
            ("cross", CargoOperation::Rustc, "cross", vec!["rustc"]),
            ("my-cross", CargoOperation::Rustc, "my-cross", vec!["rustc"]),
            (
                "cargo xwin rustc",
                CargoOperation::Build,
                "cargo",
                vec!["xwin", "rustc"],
            ),
        ];
        for (value, operation, program, args) in cases {
            let (actual_program, actual_args, toolchain_env) =
                invocation(&parse(value), operation, None);
            assert_eq!(actual_program, program, "{value}");
            assert_eq!(actual_args, args, "{value}");
            assert_eq!(toolchain_env, None, "{value}");
        }
    }

    #[test]
    fn passes_toolchain_selector_as_argument_only_to_cargo() {
        let (_, args, toolchain_env) = invocation(
            &CargoBuildCommand::Cargo,
            CargoOperation::Rustc,
            Some("+nightly"),
        );
        assert_eq!(args, ["+nightly", "rustc"]);
        assert_eq!(toolchain_env, None);

        let (_, args, toolchain_env) = invocation(
            &CargoBuildCommand::Cross,
            CargoOperation::Rustc,
            Some("+nightly"),
        );
        assert_eq!(args, ["rustc"]);
        assert_eq!(toolchain_env.as_deref(), Some("nightly"));

        let (_, args, toolchain_env) = invocation(
            &CargoBuildCommand::Zigbuild,
            CargoOperation::Build,
            Some("+1.97.1"),
        );
        assert_eq!(args, ["zigbuild"]);
        assert_eq!(toolchain_env.as_deref(), Some("1.97.1"));
    }
}

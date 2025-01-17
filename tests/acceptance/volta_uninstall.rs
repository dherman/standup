//! Tests for `volta uninstall`.

use crate::support::sandbox::{sandbox, Sandbox};
use cfg_if::cfg_if;
use hamcrest2::assert_that;
use hamcrest2::prelude::*;
use test_support::matchers::execs;

fn platform_with_node_npm(node: &str, npm: &str) -> String {
    format!(
        r#"{{
  "node": {{
    "runtime": "{}",
    "npm": "{}"
  }},
  "pnpm": null,
  "yarn": null
}}"#,
        node, npm
    )
}
fn node_bin(version: &str) -> String {
    cfg_if! {
            if #[cfg(target_os = "windows")] {
                format!(
                    r#"@echo off
echo Node version {}
echo node args: %*
"#,
    version
                )
            } else {
                format!(
                    r#"#!/bin/sh
echo "Node version {}"
echo "node args: $@"
"#,
    version
                )
            }
        }
}

const PKG_CONFIG_BASIC: &str = r#"{
  "name": "cowsay",
  "version": "1.4.0",
  "platform": {
    "node": "11.10.1",
    "npm": "6.7.0",
    "yarn": null
  },
  "bins": [
    "cowsay",
    "cowthink"
  ],
  "manager": "Npm"
}"#;

const PKG_CONFIG_NO_BINS: &str = r#"{
  "name": "cowsay",
  "version": "1.4.0",
  "platform": {
    "node": "11.10.1",
    "npm": "6.7.0",
    "yarn": null
  },
  "bins": [],
  "manager": "Npm"
}"#;

fn bin_config(name: &str) -> String {
    format!(
        r#"{{
  "name": "{}",
  "package": "cowsay",
  "version": "1.4.0",
  "platform": {{
    "node": "11.10.1",
    "npm": "6.7.0",
    "yarn": null
  }},
  "manager": "Npm"
}}"#,
        name
    )
}

const VOLTA_LOGLEVEL: &str = "VOLTA_LOGLEVEL";

#[test]
fn uninstall_nonexistent_pkg() {
    // if the package doesn't exist, it should just inform the user but not throw an error
    let s = sandbox().env(VOLTA_LOGLEVEL, "info").build();

    assert_that!(
        s.volta("uninstall cowsay"),
        execs()
            .with_status(0)
            .with_stderr_contains("[..]No package 'cowsay' found to uninstall")
    );
}

#[test]
fn uninstall_package_basic() {
    // basic uninstall - everything exists, and everything except the cached
    // inventory files should be deleted
    let s = sandbox()
        .package_config("cowsay", PKG_CONFIG_BASIC)
        .binary_config("cowsay", &bin_config("cowsay"))
        .binary_config("cowthink", &bin_config("cowthink"))
        .shim("cowsay")
        .shim("cowthink")
        .package_image("cowsay", "1.4.0", None)
        .env(VOLTA_LOGLEVEL, "info")
        .build();

    assert_that!(
        s.volta("uninstall cowsay"),
        execs()
            .with_status(0)
            .with_stdout_contains("Removed executable 'cowsay' installed by 'cowsay'")
            .with_stdout_contains("Removed executable 'cowthink' installed by 'cowsay'")
            .with_stdout_contains("[..]package 'cowsay' uninstalled")
    );

    // check that everything is deleted
    assert!(!Sandbox::package_config_exists("cowsay"));
    assert!(!Sandbox::bin_config_exists("cowsay"));
    assert!(!Sandbox::bin_config_exists("cowthink"));
    assert!(!Sandbox::shim_exists("cowsay"));
    assert!(!Sandbox::shim_exists("cowthink"));
    assert!(!Sandbox::package_image_exists("cowsay"));
}

// The setup here is the same as the above, but here we check to make sure that
// if the user supplies a version, we error correctly.
#[test]
fn uninstall_package_basic_with_version() {
    // basic uninstall - everything exists, and everything except the cached
    // inventory files should be deleted
    let s = sandbox()
        .package_config("cowsay", PKG_CONFIG_BASIC)
        .binary_config("cowsay", &bin_config("cowsay"))
        .binary_config("cowthink", &bin_config("cowthink"))
        .shim("cowsay")
        .shim("cowthink")
        .package_image("cowsay", "1.4.0", None)
        .env(VOLTA_LOGLEVEL, "info")
        .build();

    assert_that!(
        s.volta("uninstall cowsay@1.4.0"),
        execs().with_status(1).with_stderr_contains(
            "[..]error: uninstalling specific versions of tools is not supported yet."
        )
    );
}

#[test]
fn uninstall_package_no_bins() {
    // the package doesn't contain any executables, it should uninstall without error
    // (normally installing a package with no executables should not happen)
    let s = sandbox()
        .package_config("cowsay", PKG_CONFIG_NO_BINS)
        .package_image("cowsay", "1.4.0", None)
        .env(VOLTA_LOGLEVEL, "info")
        .build();

    assert_that!(
        s.volta("uninstall cowsay"),
        execs()
            .with_status(0)
            .with_stdout_contains("[..]package 'cowsay' uninstalled")
    );

    // check that everything is deleted
    assert!(!Sandbox::package_config_exists("cowsay"));
    assert!(!Sandbox::bin_config_exists("cowsay"));
    assert!(!Sandbox::bin_config_exists("cowthink"));
    assert!(!Sandbox::shim_exists("cowsay"));
    assert!(!Sandbox::shim_exists("cowthink"));
    assert!(!Sandbox::package_image_exists("cowsay"));
}

#[test]
fn uninstall_package_no_image() {
    // there is no unpacked & initialized package, but everything should be removed
    // (without erroring and failing to remove everything)
    let s = sandbox()
        .package_config("cowsay", PKG_CONFIG_BASIC)
        .binary_config("cowsay", &bin_config("cowsay"))
        .binary_config("cowthink", &bin_config("cowthink"))
        .shim("cowsay")
        .shim("cowthink")
        .env(VOLTA_LOGLEVEL, "info")
        .build();

    assert_that!(
        s.volta("uninstall cowsay"),
        execs()
            .with_status(0)
            .with_stdout_contains("Removed executable 'cowsay' installed by 'cowsay'")
            .with_stdout_contains("Removed executable 'cowthink' installed by 'cowsay'")
            .with_stdout_contains("[..]package 'cowsay' uninstalled")
    );

    // check that everything is deleted
    assert!(!Sandbox::package_config_exists("cowsay"));
    assert!(!Sandbox::bin_config_exists("cowsay"));
    assert!(!Sandbox::bin_config_exists("cowthink"));
    assert!(!Sandbox::shim_exists("cowsay"));
    assert!(!Sandbox::shim_exists("cowthink"));
    assert!(!Sandbox::package_image_exists("cowsay"));
}

#[test]
fn uninstall_package_orphaned_bins() {
    // the package config does not exist, but for some reason there are orphaned binaries
    // those should be removed
    let s = sandbox()
        .binary_config("cowsay", &bin_config("cowsay"))
        .binary_config("cowthink", &bin_config("cowthink"))
        .shim("cowsay")
        .shim("cowthink")
        .env(VOLTA_LOGLEVEL, "info")
        .build();

    assert_that!(
        s.volta("uninstall cowsay"),
        execs()
            .with_status(0)
            .with_stdout_contains("Removed executable 'cowsay' installed by 'cowsay'")
            .with_stdout_contains("Removed executable 'cowthink' installed by 'cowsay'")
            .with_stdout_contains("[..]package 'cowsay' uninstalled")
    );

    // check that everything is deleted
    assert!(!Sandbox::bin_config_exists("cowsay"));
    assert!(!Sandbox::bin_config_exists("cowthink"));
    assert!(!Sandbox::shim_exists("cowsay"));
    assert!(!Sandbox::shim_exists("cowthink"));
}

#[test]
fn uninstall_nonexistent_runtime() {
    let s = sandbox().env(VOLTA_LOGLEVEL, "info").build();
    assert_that!(
        s.volta("uninstall node@20.16.0"),
        execs()
            .with_status(0)
            .with_stderr_contains("[..]No version 'node@20.16.0' found to uninstall")
    )
}

#[test]
fn uninstall_runtime_basic() {
    // basic uninstall - everything exists, and everything except the cached
    // inventory files should not be deleted
    let s = sandbox()
        .platform(&platform_with_node_npm("20.16.0", "10.8.1"))
        .setup_node_binary("20.16.0", "10.8.1", &node_bin("20.16.0"))
        .env(VOLTA_LOGLEVEL, "info")
        .build();

    assert_that!(
        s.volta("uninstall node@20.16.0"),
        execs()
            .with_status(0)
            .with_stdout_contains("[..]'node@20.16.0' uninstalled")
    );
}

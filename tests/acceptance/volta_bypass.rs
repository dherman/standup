use crate::support::sandbox::{sandbox, shim_exe};
use hamcrest2::assert_that;
use hamcrest2::prelude::*;
use test_support::matchers::execs;

use volta_core::error::ExitCode;

#[test]
fn shim_skips_platform_checks_on_bypass() {
    let s = sandbox()
        .env("VOLTA_BYPASS", "1")
        .env(
            "VOLTA_INSTALL_DIR",
            &shim_exe().parent().unwrap().to_string_lossy(),
        )
        .build();

    assert_that!(
        s.process(shim_exe()),
        execs()
            .with_status(ExitCode::ExecutionFailure as i32)
            .with_stderr_contains("VOLTA_BYPASS is enabled[..]")
    );
}

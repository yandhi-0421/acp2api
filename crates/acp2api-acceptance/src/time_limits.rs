use std::{sync::OnceLock, time::Duration};

pub fn real_limit(base: Duration) -> Duration {
    static FACTOR: OnceLock<u32> = OnceLock::new();
    base * *FACTOR.get_or_init(|| match std::env::var_os("ACP2API_TEST_TIME_FACTOR") {
        None => 5,
        Some(value) => value
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|factor| (1..=20).contains(factor))
            .expect("ACP2API_TEST_TIME_FACTOR must be a whole number from 1 to 20"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn setting_uses_default_validates_and_rejects_unusable_values() {
        if let Ok(seconds) = std::env::var("ACP2API_TEST_EXPECTED_SECONDS") {
            assert_eq!(
                real_limit(Duration::from_secs(2)),
                Duration::from_secs(seconds.parse().unwrap())
            );
            return;
        }
        for (value, seconds, succeeds) in [
            (None::<&str>, "10", true),
            (Some("1"), "2", true),
            (Some("20"), "40", true),
            (Some("0"), "0", false),
            (Some("21"), "0", false),
            (Some("five"), "0", false),
            (Some("1.5"), "0", false),
            (Some(""), "0", false),
        ] {
            let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "time_limits::tests::setting_uses_default_validates_and_rejects_unusable_values",
                    "--nocapture",
                ])
                .env_remove("ACP2API_TEST_TIME_FACTOR")
                .env("ACP2API_TEST_EXPECTED_SECONDS", seconds)
                .env_remove("RUST_BACKTRACE")
                .kill_on_drop(true);
            if let Some(value) = value {
                command.env("ACP2API_TEST_TIME_FACTOR", value);
            }
            let output =
                tokio::time::timeout(real_limit(Duration::from_secs(10)), command.output())
                    .await
                    .expect("setting test child cap")
                    .unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            if !succeeds {
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("ACP2API_TEST_TIME_FACTOR")
                );
            }
        }
    }
}

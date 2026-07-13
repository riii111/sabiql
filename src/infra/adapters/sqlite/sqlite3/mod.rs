mod error;
mod executor;
mod metadata;
pub(super) mod parser;

pub(super) use executor::SqliteCli;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};

    use tokio::process::Command;

    use super::super::SqliteAdapter;

    #[derive(Default)]
    struct TestCommandConfig {
        environment: Vec<(OsString, OsString)>,
        process_count: usize,
    }

    static COMMAND_CONFIGS: OnceLock<Mutex<HashMap<String, TestCommandConfig>>> = OnceLock::new();

    fn command_configs() -> &'static Mutex<HashMap<String, TestCommandConfig>> {
        COMMAND_CONFIGS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub(super) fn configure_command(path: &str, command: &mut Command) {
        let mut configs = command_configs()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(config) = configs.get_mut(path) else {
            return;
        };
        config.process_count += 1;
        for (key, value) in &config.environment {
            command.env(key, value);
        }
    }

    pub(super) struct TestCommandContext {
        path: String,
    }

    impl TestCommandContext {
        pub(super) fn count(&self) -> usize {
            command_configs()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&self.path)
                .map_or(0, |config| config.process_count)
        }
    }

    impl Drop for TestCommandContext {
        fn drop(&mut self) {
            command_configs()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&self.path);
        }
    }

    impl SqliteAdapter {
        pub(super) fn with_process_counter(dsn: &str) -> (Self, TestCommandContext) {
            Self::with_test_command_config(dsn, TestCommandConfig::default())
        }

        pub(super) fn with_test_environment(
            dsn: &str,
            environment: Vec<(OsString, OsString)>,
        ) -> (Self, TestCommandContext) {
            Self::with_test_command_config(
                dsn,
                TestCommandConfig {
                    environment,
                    process_count: 0,
                },
            )
        }

        fn with_test_command_config(
            dsn: &str,
            config: TestCommandConfig,
        ) -> (Self, TestCommandContext) {
            let path = Self::path_from_dsn(dsn).unwrap().to_string();
            let previous = command_configs()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(path.clone(), config);
            assert!(previous.is_none());
            (Self::new(), TestCommandContext { path })
        }
    }
}

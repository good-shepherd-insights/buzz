// Compile and run the lifecycle state-machine contract as an integration target.
// settings.rs cannot be path-included (it depends on the rest of the crate), so
// this target supplies the two values pool_lifecycle.rs reads, matching
// buzz-acp.settings.toml [pool_lifecycle].
#[allow(dead_code)]
mod settings {
    use std::time::Duration;

    pub struct PoolLifecycleSettings {
        pub initial_retry_delay_secs: Duration,
        pub max_retry_delay_secs: Duration,
    }

    pub struct Settings {
        pub pool_lifecycle: PoolLifecycleSettings,
    }

    pub fn get() -> &'static Settings {
        static S: Settings = Settings {
            pool_lifecycle: PoolLifecycleSettings {
                initial_retry_delay_secs: Duration::from_secs(5),
                max_retry_delay_secs: Duration::from_secs(300),
            },
        };
        &S
    }

    pub fn init_for_tests() {}
}

#[allow(dead_code)]
#[path = "../src/pool_lifecycle.rs"]
mod pool_lifecycle;

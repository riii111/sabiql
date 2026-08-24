mod adapter;
mod cli;
mod connection;
mod dsn;
mod executor;
mod metadata;
mod option_file;
mod sql;

#[cfg(feature = "test-support")]
pub mod test_support;

#[cfg(test)]
pub(in crate::adapters::mysql) mod test_query_assertions {
    pub(in crate::adapters::mysql) fn assert_queries_in_order(log: &str, queries: &[&str]) {
        let positions = queries
            .iter()
            .map(|query| log.find(*query).expect("query in transcript"))
            .collect::<Vec<_>>();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
    }
}

pub use adapter::MySqlAdapter;

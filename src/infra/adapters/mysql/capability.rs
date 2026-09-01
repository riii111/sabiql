#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MySqlServerCapabilities {
    pub(super) lower_case_table_names: u8,
    major: u32,
    minor: u32,
    patch: u32,
}

impl MySqlServerCapabilities {
    pub(super) fn from_version(version: &str, lower_case_table_names: u8) -> Self {
        let mut numbers = version
            .split(|character: char| !character.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<u32>().ok());
        Self {
            major: numbers.next().unwrap_or_default(),
            minor: numbers.next().unwrap_or_default(),
            patch: numbers.next().unwrap_or_default(),
            lower_case_table_names,
        }
    }

    pub(super) fn supports_generation_expression(self) -> bool {
        self.at_least(5, 7, 6)
    }

    pub(super) fn supports_statistics_expression(self) -> bool {
        self.at_least(8, 0, 13)
    }

    pub(super) fn supports_statistics_visibility(self) -> bool {
        self.at_least(8, 0, 0)
    }

    pub(super) fn supports_common_table_expressions(self) -> bool {
        self.at_least(8, 0, 1)
    }

    fn at_least(self, major: u32, minor: u32, patch: u32) -> bool {
        (self.major, self.minor, self.patch) >= (major, minor, patch)
    }
}

impl Default for MySqlServerCapabilities {
    fn default() -> Self {
        Self::from_version("8.4.10", 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_capabilities_at_mysql_version_boundaries() {
        let mysql_57 = MySqlServerCapabilities::from_version("5.7.44", 0);
        assert!(mysql_57.supports_generation_expression());
        assert!(!mysql_57.supports_statistics_expression());
        assert!(!mysql_57.supports_statistics_visibility());
        assert!(!mysql_57.supports_common_table_expressions());

        let mysql_80 = MySqlServerCapabilities::from_version("8.0.12", 1);
        assert!(mysql_80.supports_generation_expression());
        assert!(!mysql_80.supports_statistics_expression());
        assert!(mysql_80.supports_statistics_visibility());
        assert!(mysql_80.supports_common_table_expressions());

        let mysql_801 = MySqlServerCapabilities::from_version("8.0.1", 1);
        assert!(mysql_801.supports_common_table_expressions());

        let mysql_84 = MySqlServerCapabilities::from_version("8.4.10", 0);
        assert!(mysql_84.supports_statistics_expression());
        assert!(mysql_84.supports_statistics_visibility());
        assert!(mysql_84.supports_common_table_expressions());
    }

    #[test]
    fn omits_features_for_versions_without_verifiable_components() {
        let unknown = MySqlServerCapabilities::from_version("unknown", 2);
        assert_eq!(unknown.lower_case_table_names, 2);
        assert!(!unknown.supports_generation_expression());
        assert!(!unknown.supports_statistics_expression());
        assert!(!unknown.supports_statistics_visibility());
        assert!(!unknown.supports_common_table_expressions());
    }
}

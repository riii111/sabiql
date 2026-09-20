#!/usr/bin/env ruby
# frozen_string_literal: true

require "minitest/autorun"
require "open3"
require "rbconfig"
require "tmpdir"

SCRIPT = File.expand_path("lint_test_names.rb", __dir__)

class LintTestNamesTest < Minitest::Test
  def run_lint(source)
    Dir.mktmpdir do |directory|
      path = File.join(directory, "fixture.rs")
      File.write(path, source)
      return Open3.capture3(RbConfig.ruby, SCRIPT, path)
    end
  end

  def test_allows_test_only_items_inside_test_modules
    stdout, stderr, status = run_lint(<<~RUST)
      #[cfg(test)]
      mod tests {
          #[cfg(test)]
          fn helper() {}

          #[cfg(test)]
          #[allow(dead_code)]
          fn stacked_helper() {}

          #[cfg(test)]
          struct Fixture;

          #[cfg(test)]
          impl Fixture {}

          #[cfg(test)]
          const VALUE: usize = 1;

          #[cfg(test)]
          type Alias = usize;

          #[cfg(test)]
          static SHARED: usize = 1;

          #[cfg(test)]
          #[allow(unused_imports)]
          use crate::{
              Fixture,
              OtherFixture,
          };
      }

      #[cfg(any(test, feature = "test-support"))]
      use crate::SharedFixture;

      #[cfg(feature = "test-support")]
      use crate::FeatureFixture;

      #[cfg(feature = "test-support")]
      fn test_support_helper() {}
    RUST

    assert status.success?, stderr
    assert_equal "test-name lint passed\n", stdout
    assert_empty stderr
  end

  def test_rejects_test_only_items_at_module_scope
    _stdout, stderr, status = run_lint(<<~RUST)
      #[cfg(test)]
      fn misplaced_function() {}

      #[cfg(test)]
      extern "C" fn misplaced_extern_function() {}

      #[cfg(test)]
      async unsafe fn misplaced_async_unsafe_function() {}

      #[cfg(test)]
      struct MisplacedStruct;

      #[cfg(test)]
      impl MisplacedStruct {}

      #[cfg(test)]
      const MISPLACED_CONST: usize = 1;

      #[cfg(test)]
      type MisplacedType = usize;

      #[cfg(test)]
      static MISPLACED_STATIC: usize = 1;

      #[cfg(test)]
      #[allow(dead_code)]
      fn stacked_misplaced_function() {}

      mod production {
          #[cfg(test)]
          fn nested_function() {}
      }
    RUST

    refute status.success?
    assert_equal 10, stderr.lines.grep(/test-only item must be inside a cfg\(test\) module/).length
    assert_includes stderr, "fixture.rs"
  end

  def test_rejects_private_test_use_at_module_scope
    _stdout, stderr, status = run_lint(<<~RUST)
      #[cfg(test)]
      #[allow(unused_imports)]
      use crate::Fixture;

      #[cfg(test)]
      use crate::{
          FirstFixture,
          SecondFixture,
      };

      mod production {
          #[cfg(test)]
          use crate::NestedFixture;
      }
    RUST

    refute status.success?
    assert_equal 3, stderr.lines.grep(/test-only use must be inside a cfg\(test\) module/).length
  end
end

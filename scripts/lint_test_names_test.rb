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
          struct Fixture;

          #[cfg(test)]
          impl Fixture {}

          #[cfg(test)]
          const VALUE: usize = 1;

          #[cfg(test)]
          type Alias = usize;

          #[cfg(test)]
          static SHARED: usize = 1;
      }

      #[cfg(test)]
      use crate::Fixture;

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

      mod production {
          #[cfg(test)]
          fn nested_function() {}
      }
    RUST

    refute status.success?
    assert_equal 9, stderr.lines.grep(/test-only item must be inside a cfg\(test\) module/).length
    assert_includes stderr, "fixture.rs"
  end

  def test_keeps_stacked_attributes_and_test_module_scope_distinct
    _stdout, stderr, status = run_lint(<<~RUST)
      #[cfg(test)]
      #[allow(dead_code)]
      fn misplaced_function() {}

      #[cfg(test)]
      mod tests {
          #[cfg(test)]
          #[allow(dead_code)]
          fn helper() {}
      }
    RUST

    refute status.success?
    assert_equal 1, stderr.lines.grep(/test-only item must be inside a cfg\(test\) module/).length
  end
end

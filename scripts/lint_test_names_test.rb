#!/usr/bin/env ruby
# frozen_string_literal: true

require "fileutils"
require "minitest/autorun"
require "open3"
require "rbconfig"
require "tmpdir"

SCRIPT = File.expand_path("lint_test_names.rb", __dir__)

class LintTestNamesTest < Minitest::Test
  def run_lint(source)
    run_lint_files("fixture.rs" => source)
  end

  def run_lint_files(sources)
    Dir.mktmpdir do |directory|
      sources.each do |name, source|
        path = File.join(directory, name)
        FileUtils.mkdir_p(File.dirname(path))
        File.write(path, source)
      end
      Open3.capture3(RbConfig.ruby, SCRIPT, directory)
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

  def test_allows_stacked_test_use_tree_inside_test_module
    stdout, stderr, status = run_lint(<<~RUST)
      #[cfg(test)]
      mod tests {
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
    RUST

    assert status.success?, stderr
    assert_equal "test-name lint passed\n", stdout
    assert_empty stderr
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

  def test_rejects_test_only_associated_and_local_items
    _stdout, stderr, status = run_lint(<<~'RUST')
      impl<const N: usize> Fixture<N>
      where
          Self: Sized,
      {
          #[cfg(test)] fn misplaced_method(&self) {}
          #[cfg(test)] pub(crate) const fn misplaced_const_method() {}
          #[cfg(test)] const VALUE: usize = N;
      }

      unsafe trait Interface {
          #[cfg(test)]
          type MisplacedType;
          #[cfg(test)]
          unsafe fn misplaced_trait_method();
      }

      unsafe extern "C" {
          #[cfg(test)] safe fn misplaced_foreign_function();
      }

      fn production() {
          #[cfg(test)]
          fn misplaced_local_function() {}

          #[cfg(test)]
          const { assert!(true); }

          if !{
              #[cfg(test)]
              fn inside_negated_block() {}
              true
          } {}
      }
    RUST

    refute status.success?
    assert_equal %w[fn fn const type fn fn fn fn], stderr.scan(/cfg\(test\) module: (\w+)/).flatten
  end

  def test_rejects_other_test_only_declarations_and_private_imports
    _stdout, stderr, status = run_lint(<<~'RUST')
      #[cfg(test)] enum MisplacedEnum {}
      #[cfg(test)] union MisplacedUnion { field: u8 }
      #[cfg(test)] trait MisplacedTrait {}
      #[cfg(test)] unsafe extern "C" {}
      #[cfg(test)] macro_rules! misplaced_macro { () => {} }

      #[cfg(
          all(unix, /* platform */ test,)
      )]
      #[allow(
          unused_imports
      )]
      use crate::Fixture;

      #[cfg(any(all(unix, test), all(windows, test)))]
      fn guarded_on_every_platform() {}

      #[cfg_attr(not(test), cfg(test))]
      fn gated_by_cfg_attr() {}

      #[r#cfg(r#all(r#test))]
      fn raw_cfg() {}
    RUST

    refute status.success?
    assert_equal %w[enum union trait extern macro_rules fn fn fn], stderr.scan(/cfg\(test\) module: (\w+)/).flatten
    assert_equal 1, stderr.lines.grep(/test-only use must be inside a cfg\(test\) module/).length
  end

  def test_allows_test_modules_and_shared_items
    stdout, stderr, status = run_lint(<<~'RUST')
      #[cfg(/* platform */ all(test, unix,))]
      #[path = "test_support.rs"]
      pub(crate) mod test_support;

      #[cfg(all(test, unix))]
      mod support
      {
          #[cfg(test)]
          macro_rules! fixture { () => {} }

          mod nested { impl Fixture { #[cfg(test)] fn helper() {} } }
          #[cfg(test)] use crate::Fixture;
      }

      mod self_gated {
          #![cfg(test)]
          #[cfg(test)] fn fixture() {}
      }

      #[cfg(any(test, feature = "test-support"))]
      fn shared_helper() {}

      #[cfg(any(test, unix))]
      fn native_helper() {}

      #[cfg(not(test))]
      fn production_helper() {}

      #[cfg_attr(test, allow(dead_code))]
      fn helper_with_test_specific_attribute() {}

      #[cfg(test)] pub(crate) use crate::fixture::Fixture;
    RUST

    assert status.success?, stderr
    assert_equal "test-name lint passed\n", stdout
    assert_empty stderr
  end

  def test_ignores_attribute_like_text_in_comments_literals_and_macros
    stdout, stderr, status = run_lint(<<~'RUST')
      const RAW: &str = r##"
      #[cfg(test)]
      fn in_raw_string() {}
      "##;
      const STRING: &str = "
      #[cfg(test)]
      fn in_string() {}
      ";
      const CHAR: char = '\u{7b}';
      /* #[cfg(test)]
         /* nested */
         fn in_comment() {} */
      macro_rules! declarations {
          () => {
              #[cfg(test)]
              fn in_macro_template() {}
          };
      }
      declarations! {
          #[cfg(test)]
          fn in_macro_arguments() {}
      }
      struct Fixture {
          #[cfg(test)]
          extra_field: usize,
      }
      enum Choice {
          #[cfg(test)]
          ExtraVariant,
      }
    RUST

    assert status.success?, stderr
    assert_equal "test-name lint passed\n", stdout
    assert_empty stderr
  end

  def test_allows_items_in_out_of_line_test_modules
    entry = <<~'RUST'
      #[cfg(test)] mod tests;
      #[cfg_attr(test, path = "conditional_test.rs")] mod platform;
      #[cfg(test)] mod inline {
          #[path = "fïxture\x2ers"] mod helper;
      }
    RUST
    tests = <<~'RUST'
      mod nested;
      #[path = r#"aliased.rs"#] mod fixture;
      impl Fixture { #[cfg(test)] fn helper() {} }
    RUST
    aliased = <<~'RUST'
      mod child;
      #[cfg(test)] struct Fixture;
    RUST
    helper = "#[cfg(test)] fn helper() {}\n"
    stdout, stderr, status = run_lint_files(
      "lib.rs" => entry,
      "tests.rs" => tests,
      "tests/nested/mod.rs" => helper,
      "aliased.rs" => aliased,
      "child.rs" => helper,
      "platform.rs" => "fn production() {}\n",
      "conditional_test.rs" => helper,
      "inline/fïxture.rs" => helper,
    )

    assert status.success?, stderr
    assert_equal "test-name lint passed\n", stdout
    assert_empty stderr
  end

  def test_rejects_items_in_a_file_also_loaded_by_production
    entry = <<~'RUST'
      #[cfg(test)] #[path = "shared.rs"] mod tests;
      #[cfg_attr(not(test), path = "shared.rs")] mod production;
    RUST
    _stdout, stderr, status = run_lint_files(
      "lib.rs" => entry,
      "production.rs" => "fn production() {}\n",
      "shared.rs" => "#[cfg(test)] fn misplaced() {}\n",
    )

    refute status.success?
    assert_match(/shared\.rs:1 test-only item must be inside a cfg\(test\) module: fn/, stderr)
  end

  def test_reports_after_a_test_module_closes_and_preserves_name_checks
    _stdout, stderr, status = run_lint(<<~'RUST')
      #[cfg(test)] mod tests { mod helpers { #[cfg(test)] fn fixture() {} } }
      mod production { #[cfg(test)] fn misplaced() {} }

      #[cfg(test)]
      mod query_tests {
          #[rstest]
          #[case(
              1
          )]
          fn query_returns_expected(#[case] n: usize) {}
          #[tokio::test] async fn duplicate() {}
          #[test] fn duplicate() {}
      }
      #[cfg(test)] mod other { #[test] fn duplicate() {} }
    RUST

    refute status.success?
    diagnostics = stderr.lines.grep(/^- /)
    assert_equal 4, diagnostics.length
    assert_match(/fixture.rs:2 test-only item/, diagnostics[0])
    assert_match(/fixture.rs:10 test name must not contain/, diagnostics[1])
    assert_match(/fixture.rs:10 redundant module prefix/, diagnostics[2])
    assert_match(/fixture.rs:12 duplicate test name.*first seen at line 11/, diagnostics[3])
  end
end

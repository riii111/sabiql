#!/usr/bin/env ruby
# frozen_string_literal: true

require "strscan"

ROOT = File.expand_path("..", __dir__)
DEFAULT_PATHS = ["src", "tests"].freeze

def rust_files(paths)
  patterns = paths.flat_map do |path|
    absolute = File.expand_path(path, ROOT)
    File.directory?(absolute) ? File.join(absolute, "**/*.rs") : absolute
  end

  patterns.flat_map { |pattern| Dir.glob(pattern) }.sort.uniq
end

# Keep Rust token trees balanced without interpreting Rust expressions or expanding macros.
class RustTokens
  IDENTIFIER = /(?:r\#)?[_\p{XID_Start}][\p{XID_Continue}]*/u
  CHAR_LITERAL = /'(?:\\(?:u\{[\da-fA-F_]+\}|x[\da-fA-F]{2}|[^\r\n])|[^'\\\r\n])'/
  GROUP_END = { "(" => ")", "[" => "]", "{" => "}" }.freeze
  Token = Struct.new(:text, :line, :after_group)

  def initialize(source)
    @tokens = []
    @scanner = StringScanner.new(source)
    line = 1
    groups = []

    until @scanner.eos?
      if @scanner.scan(%r{\s+|//[^\n]*})
        line += @scanner.matched.count("\n")
      elsif @scanner.scan(%r{/\*})
        line += skip_block_comment
      else
        start = @scanner.pos
        text = scan_token
        token = Token.new(text, line)
        line += @scanner.string.byteslice(start...@scanner.pos).count("\n")
        @tokens << token

        if GROUP_END.key?(text)
          groups << token
        elsif groups.any? && GROUP_END[groups.last.text] == text
          groups.pop.after_group = @tokens.length
        end
      end
    end
  end

  def [](index)
    @tokens[index]&.text
  end

  def line(index)
    @tokens[index].line
  end

  def after_group(index)
    @tokens[index]&.after_group
  end

  private

  def scan_token
    if @scanner.scan(/(?:b|c)?r(\#*)"/)
      terminator = '"' + @scanner[1]
      @scanner.terminate unless @scanner.scan_until(/#{Regexp.escape(terminator)}/)
      '"'
    elsif @scanner.scan(/(?:b|c)?"(?:\\.|[^"\\])*"/m)
      '"'
    elsif @scanner.scan(CHAR_LITERAL)
      "'"
    else
      @scanner.scan(IDENTIFIER) || @scanner.getch
    end
  end

  def skip_block_comment
    depth = 1
    newlines = 0
    while depth.positive? && (part = @scanner.scan_until(%r{/\*|\*/}))
      newlines += part.count("\n")
      depth += part.end_with?("/*") ? 1 : -1
    end
    @scanner.terminate if depth.positive?
    newlines
  end
end

class RustTestLint
  ITEM_KINDS = %w[fn struct enum union trait impl const type static macro_rules macro].freeze
  ModuleTests = Struct.new(:name, :names)

  attr_reader :errors

  def initialize(source, file)
    @tokens = RustTokens.new(source)
    @file = file
    @errors = []
  end

  def run
    lint_scope(0, ModuleTests.new(nil, {}), false, module_body: true)
    errors
  end

  private

  def lint_scope(index, module_tests, inside_test_module, module_body: false)
    inner_attributes, index = read_attributes(index, inner: true)
    inside_test_module ||= module_body && test_only?(inner_attributes)

    while (text = @tokens[index]) && text != "}"
      attributes, item = read_attributes(index)
      keyword, visibility = item_keyword(item)
      kind = @tokens[keyword]
      body = keyword + 2

      if kind == "mod" && @tokens[body] == "{"
        name = @tokens[keyword + 1].delete_prefix("r#")
        test_module = inside_test_module || test_only?(attributes)
        lint_scope(body + 1, ModuleTests.new(name, {}), test_module, module_body: true)
        index = @tokens.after_group(body)
        next
      end

      unless attributes.empty?
        lint_placement(attributes, keyword, visibility) unless inside_test_module
        lint_name(attributes, keyword, module_tests) if kind == "fn"
      end

      # Skip macro token trees and attribute/argument groups. Ordinary Rust blocks can contain local items.
      if @tokens[item] == "{"
        lint_scope(item + 1, module_tests, inside_test_module)
      end
      index = after_macro(item, keyword) || @tokens.after_group(item) || item + 1
    end
  end

  def after_macro(item, keyword)
    case @tokens[keyword]
    when "macro_rules"
      @tokens.after_group(keyword + 3) if @tokens[keyword + 1] == "!"
    when "macro"
      body = keyword + 2
      body = @tokens.after_group(body) if @tokens[body] == "("
      @tokens.after_group(body)
    else
      if @tokens[item + 1] == "!" && @tokens[item]&.match?(RustTokens::IDENTIFIER)
        @tokens.after_group(item + 2)
      end
    end
  end

  def read_attributes(index, inner: false)
    attributes = []
    opening = index + (inner ? 2 : 1)
    while @tokens[index] == "#" && (!inner || @tokens[index + 1] == "!") && @tokens[opening] == "["
      attributes << opening + 1
      index = @tokens.after_group(opening)
      opening = index + (inner ? 2 : 1)
    end
    [attributes, index]
  end

  def item_keyword(index)
    visibility = @tokens[index] == "pub"
    if visibility
      index += 1
      index = @tokens.after_group(index) if @tokens[index] == "("
    end

    loop do
      case @tokens[index]
      when "unsafe", "async", "default", "safe"
        index += 1
      when "const"
        break unless %w[fn unsafe async extern].include?(@tokens[index + 1])

        index += 1
      when "extern"
        following = index + (@tokens[index + 1] == '"' ? 2 : 1)
        break unless @tokens[following] == "fn"

        index = following
      else
        break
      end
    end
    [index, visibility]
  end

  def lint_placement(attributes, keyword, visibility)
    return unless test_only?(attributes)

    kind = @tokens[keyword]
    if kind == "use" && !visibility
      report(keyword, "test-only use must be inside a cfg(test) module")
    elsif kind == "extern" || (ITEM_KINDS.include?(kind) && @tokens[keyword + 1] != "{")
      report(keyword, "test-only item must be inside a cfg(test) module: #{kind}")
    end
  end

  def test_only?(attributes)
    attributes.any? { |attribute| cfg_attribute(attribute, false) == false } &&
      attributes.none? { |attribute| cfg_attribute(attribute, true) == false }
  end

  # Evaluate with test fixed and every target/feature predicate unknown.
  def cfg_attribute(index, test)
    return true unless @tokens[index + 1] == "("

    case @tokens[index]
    when "cfg"
      cfg_predicate(index + 2, test)
    when "cfg_attr"
      condition, *attributes = arguments(index + 1)
      result = all(attributes.map { |attribute| cfg_attribute(attribute, test) })
      any([invert(cfg_predicate(condition, test)), result])
    else
      true
    end
  end

  def cfg_predicate(index, test)
    case @tokens[index]
    when "test"
      @tokens[index + 1] == "=" ? nil : test
    when "true"
      true
    when "false"
      false
    when "all", "any", "not"
      return nil unless @tokens[index + 1] == "("

      values = arguments(index + 1).map { |argument| cfg_predicate(argument, test) }
      case @tokens[index]
      when "all" then all(values)
      when "any" then any(values)
      when "not" then invert(values.first)
      end
    end
  end

  def arguments(opening)
    arguments = []
    closing = @tokens.after_group(opening) - 1
    index = opening + 1
    while index < closing
      arguments << index
      while index < closing && @tokens[index] != ","
        index = @tokens.after_group(index) || index + 1
      end
      index += 1
    end
    arguments
  end

  def all(values)
    return false if values.include?(false)

    true unless values.include?(nil)
  end

  def any(values)
    return true if values.include?(true)

    false unless values.include?(nil)
  end

  def invert(value)
    !value unless value.nil?
  end

  def lint_name(attributes, keyword, module_tests)
    return unless attributes.any? { |index| test_attribute?(index) }

    name = @tokens[keyword + 1].delete_prefix("r#")
    if name.include?("returns_expected")
      report(keyword, "test name must not contain `returns_expected`: #{name} (use a concrete result, drop the suffix if the result is already implied, or split the rstest if cases do not share one guarantee)")
    end

    if (previous = module_tests.names[name])
      report(keyword, "duplicate test name in same module: #{name} (first seen at line #{previous})")
    else
      module_tests.names[name] = @tokens.line(keyword)
    end

    prefix = module_tests.name&.sub(/_tests?\z/, "")
    if prefix && prefix != "tests" && !prefix.empty? && name.start_with?("#{prefix}_")
      report(keyword, "redundant module prefix `#{prefix}_` in test name: #{name}")
    end
  end

  def test_attribute?(index)
    if @tokens[index] == "tokio" && @tokens[index + 1] == ":" && @tokens[index + 2] == ":"
      return @tokens[index + 3] == "test" && @tokens[index + 4] != ":"
    end

    %w[test rstest].include?(@tokens[index]) && @tokens[index + 1] != ":"
  end

  def report(index, message)
    errors << "#{@file}:#{@tokens.line(index)} #{message}"
  end
end

paths = ARGV.empty? ? DEFAULT_PATHS : ARGV
errors = rust_files(paths).flat_map do |file|
  RustTestLint.new(File.read(file), file.delete_prefix("#{ROOT}/")).run
end

if errors.empty?
  puts "test-name lint passed"
else
  warn "test-name lint failed:"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
